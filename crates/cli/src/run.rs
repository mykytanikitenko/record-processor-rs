//! Command-line wiring: parse arguments, build the runtime, and drive the
//! payments engine to stdout with cooperative cancellation.

use allocator_api2::alloc::Global;
use allocator_api2::boxed::Box;
use futures_util::future::Either;
use service::engine_state::KvState;
use service::{ClientId, EngineParts, ServiceState, State, Transaction, TxId, TxRecord};
use std::hash::RandomState;
use storage::file_kv::{
    FileKv, FileKvLoadError, FileKvNextKeyError, FileKvOpenError, FileKvStoreError,
    Parts as FileKvParts,
};
use storage::file_queue::{FileQueue, OpenError, Parts as FileQueueParts};
use tokio_util::sync::CancellationToken;

/// Byte length of the transaction queue's fixed backing reservation.
///
/// The queue maps this many bytes exactly once and never grows, so every
/// appended `Transaction` must fit within it; an append that would overflow it
/// fails with `AppendError::Capacity`. Two gibibytes accommodates
/// gigabyte-scale CSV input while keeping the virtual mapping cheap on 64-bit
/// targets.
const QUEUE_CAPACITY: usize = 1 << 31;

/// Byte length of each key-value store's fixed backing reservation.
///
/// Each store appends one fixed-size record per write, and the engine writes
/// roughly once per transaction, so the stores are sized to match the queue's
/// two-gibibyte reservation; the mapping is virtual and committed lazily, so
/// the reservation is cheap until records are actually written.
const KV_CAPACITY: usize = 1 << 31;

/// The memory-mapped, file-backed store of each client's account state.
type Clients = FileKv<ClientId, ServiceState, RandomState, Global>;
/// The memory-mapped, file-backed store of each transaction's lifecycle.
type Records = FileKv<TxId, TxRecord, RandomState, Global>;
/// The memory-mapped, file-backed transaction queue.
type Queue = FileQueue<Transaction, Global>;
/// The payments engine over the memory-mapped queue and file-backed state.
type Engine = service::Engine<Queue, MemoryState>;

/// The engine state: per-client account state and the transaction lifecycle,
/// both held in memory-mapped, file-backed stores.
#[derive(Debug)]
struct MemoryState {
    /// The store holding each client's account state.
    clients: Clients,
    /// The store holding each transaction's lifecycle.
    records: Records,
}

impl State for MemoryState {
    type ClientsError = service::ClientsError<FileKvNextKeyError, FileKvLoadError>;
    type LoadClientError = service::LoadClientError<FileKvLoadError>;
    type LoadTxError = service::LoadTxError<FileKvLoadError>;
    type StoreClientError = service::StoreClientError<FileKvStoreError>;
    type StoreTxError = service::StoreTxError<FileKvStoreError>;

    #[inline]
    async fn load_client(
        &self,
        client: ClientId,
        cancel: &CancellationToken,
    ) -> Result<Option<ServiceState>, Self::LoadClientError> {
        KvState {
            clients: &self.clients,
            transactions: &self.records,
        }
        .load_client(client, cancel)
        .await
    }

    #[inline]
    async fn load_tx(
        &self,
        tx: TxId,
        cancel: &CancellationToken,
    ) -> Result<Option<TxRecord>, Self::LoadTxError> {
        KvState {
            clients: &self.clients,
            transactions: &self.records,
        }
        .load_tx(tx, cancel)
        .await
    }

    #[inline]
    async fn next_client(
        &self,
        after: Option<ClientId>,
        cancel: &CancellationToken,
    ) -> Result<Option<(ClientId, ServiceState)>, Self::ClientsError> {
        KvState {
            clients: &self.clients,
            transactions: &self.records,
        }
        .next_client(after, cancel)
        .await
    }

    #[inline]
    async fn store_client(
        &self,
        client: ClientId,
        state: ServiceState,
        cancel: &CancellationToken,
    ) -> Result<(), Self::StoreClientError> {
        KvState {
            clients: &self.clients,
            transactions: &self.records,
        }
        .store_client(client, state, cancel)
        .await
    }

    #[inline]
    async fn store_tx(
        &self,
        tx: TxId,
        state: TxRecord,
        cancel: &CancellationToken,
    ) -> Result<(), Self::StoreTxError> {
        KvState {
            clients: &self.clients,
            transactions: &self.records,
        }
        .store_tx(tx, state, cancel)
        .await
    }
}

/// Failure while running the command-line entry point.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RunError {
    /// The command line could not be parsed.
    #[error("invalid arguments: {0}")]
    Args(#[from] pico_args::Error),
    /// The cancellation signal handler failed.
    #[error("signal handling failed: {0}")]
    Cancel(#[from] CancelError),
    /// The compio runtime could not be started.
    #[error("runtime failed: {0}")]
    Runtime(#[from] std::io::Error),
    /// Serving the input through the engine failed.
    #[error("processing failed: {0}")]
    Serve(#[from] ServeInputError),
}

/// Failure while waiting for a cancellation signal.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CancelError {
    /// The signal handler could not be registered.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Failure while serving the input through the engine.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServeInputError {
    /// The input file could not be opened.
    #[error("failed to open input: {0}")]
    Open(#[from] std::io::Error),
    /// Processing a single source failed.
    #[error(transparent)]
    Process(#[from] ProcessError),
}

/// Failure while processing a single input source.
///
/// The engine's concrete error type is parameterised over the private
/// [`MemoryState`], so it is erased into a boxed error rather than leaking the
/// composition detail through the public [`RunError`] surface.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProcessError {
    /// Opening or sizing a backing file failed.
    #[error("opening or sizing a backing file failed: {0}")]
    Backing(#[from] BackingFileError),
    /// Opening the memory-mapped transaction queue failed.
    #[error("opening the transaction queue failed: {0}")]
    Queue(#[source] OpenError),
    /// Opening a memory-mapped key-value store failed.
    #[error("opening a key-value store failed: {0}")]
    Kv(#[source] FileKvOpenError),
    /// Serving the transaction stream through the engine failed.
    #[error("serving the transaction stream failed: {0}")]
    Serve(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Failure while opening or sizing a backing file.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BackingFileError {
    /// The backing file could not be opened or sized.
    #[error("opening or sizing the backing file failed: {0}")]
    Io(#[from] std::io::Error),
    /// The reservation does not fit the backing-file length type.
    #[error("the reservation does not fit the backing-file length: {0}")]
    Length(#[from] core::num::TryFromIntError),
}

/// Wait for either SIGINT or SIGTERM, then cancel the token.
async fn cancel_on_signal(token: CancellationToken) -> Result<(), CancelError> {
    let sigint = std::pin::pin!(compio::signal::ctrl_c());
    let sigterm = std::pin::pin!(compio::signal::unix::signal(libc::SIGTERM));
    match futures_util::future::select(sigint, sigterm).await {
        Either::Left((result, _)) | Either::Right((result, _)) => result?,
    }
    token.cancel();
    Ok(())
}

/// Open and size a backing file for a storage component.
///
/// Each component maps a fixed reservation, so the backing file is created (or
/// truncated) and extended to `capacity` bytes before it is mapped; the
/// component itself never resizes it. The path is unique per process and
/// component so concurrent runs never share a mapping.
async fn open_backing_file(
    name: &str,
    capacity: usize,
) -> Result<compio::fs::File, BackingFileError> {
    // TODO(lint/no-implicit-alloc): implicit heap allocation via `format!`; use
    // a memory-mapped, reused-buffer, or borrowed alternative
    let path = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    let file = compio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .await?;
    file.set_len(u64::try_from(capacity)?).await?;
    Ok(file)
}

/// Drive the engine over `source`, projecting the resulting balances to
/// stdout.
async fn process(
    source: transport::Input<compio::fs::Stdin>,
    token: &CancellationToken,
) -> Result<(), ProcessError> {
    let clients: Clients = FileKv::try_from(FileKvParts {
        alloc: Global,
        hasher: RandomState::new(),
        file: open_backing_file("record-processor-clients", KV_CAPACITY).await?,
        capacity: KV_CAPACITY,
    })
    .map_err(ProcessError::Kv)?;
    let records: Records = FileKv::try_from(FileKvParts {
        alloc: Global,
        hasher: RandomState::new(),
        file: open_backing_file("record-processor-records", KV_CAPACITY).await?,
        capacity: KV_CAPACITY,
    })
    .map_err(ProcessError::Kv)?;
    let queue: Queue = FileQueue::try_from(FileQueueParts {
        alloc: Global,
        file: open_backing_file("record-processor-queue", QUEUE_CAPACITY).await?,
        capacity: QUEUE_CAPACITY,
    })
    .map_err(ProcessError::Queue)?;
    let state = MemoryState { clients, records };
    let engine = Engine::from(EngineParts { queue, state });
    transport::serve::<_, _, Global>(engine, source, token)
        .await
        .map_err(|error| ProcessError::Serve(Box::from(error)))?;
    Ok(())
}

/// Open the input source and drive the engine.
async fn serve_input(
    input: Option<String>,
    token: &CancellationToken,
) -> Result<(), ServeInputError> {
    if let Some(path) = input {
        let file = compio::fs::File::open(path).await?;
        process(transport::Input::<compio::fs::Stdin>::from(file), token).await?;
    } else {
        let stdin = compio::fs::stdin();
        process(transport::Input::Stream(stdin), token).await?;
    }
    Ok(())
}

/// Parses the command line and runs the payments engine.
///
/// The sole optional free argument is the input CSV path; when absent, the
/// transaction stream is read from standard input. SIGINT or SIGTERM cancels
/// the run and lets the pipeline shut down gracefully.
///
/// # Errors
///
/// Returns [`RunError::Args`] when the arguments are malformed,
/// [`RunError::Runtime`] when the runtime cannot start,
/// [`RunError::Cancel`] when signal handling cannot start, and
/// [`RunError::Serve`] when the engine pipeline fails.
#[inline]
pub fn run() -> Result<(), RunError> {
    let mut args = pico_args::Arguments::from_env();
    let input: Option<String> = args.opt_free_from_str()?;
    let runtime = compio::runtime::Runtime::new()?;
    let token = CancellationToken::new();
    let pipeline_token = token.child_token();
    runtime.block_on(async move {
        let pipeline = std::pin::pin!(serve_input(input, &pipeline_token));
        let cancel = std::pin::pin!(cancel_on_signal(token));
        match futures_util::future::select(pipeline, cancel).await {
            Either::Left((result, _)) => result.map_err(RunError::Serve),
            Either::Right((cancel_result, remaining)) => {
                cancel_result?;
                remaining.await.map_err(RunError::Serve)
            }
        }
    })
}
