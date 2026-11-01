//! Command-line wiring: parse arguments, build the runtime, and drive the
//! payments engine to stdout with cooperative cancellation.

use bumpalo::Bump;
use futures_util::future::Either;
use service::engine_state::KvState;
use service::{
    ClientId, ClientsError, EngineParts, LoadClientError, LoadTxError, OverflowError, ServiceState,
    State, StoreClientError, StoreTxError, Transaction, TxId, TxRecord,
};
use std::hash::RandomState;
use std::os::unix::ffi::OsStrExt as _;
use std::path::PathBuf;
use storage::file_kv::{
    EnumerateError, FetchError, FileKv, MapError, Parts as FileKvParts, WriteError,
};
use storage::file_queue::{
    FileAppendError, FileNextBatchError, FileQueue, OpenError, Parts as FileQueueParts,
};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use transport::{ReadBatchError, StdoutError, StdoutSinkError, Transactions, WriteBatchError};

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

/// Initial byte reservation of the arena that backs the storage index and the
/// transaction batch buffer.
///
/// The arena holds the per-key indexes and the reused transaction batch, not
/// the mapped record data itself; it grows (and never shrinks) as entries are
/// appended, so this is a warm-up hint rather than a correctness bound.
const ARENA_CAPACITY: usize = 1 << 20;

/// The memory-mapped, file-backed store of each client's account state.
type Clients<'bump> = FileKv<ClientId, ServiceState, RandomState, &'bump Bump>;
/// The memory-mapped, file-backed store of each transaction's lifecycle.
type Records<'bump> = FileKv<TxId, TxRecord, RandomState, &'bump Bump>;
/// The memory-mapped, file-backed transaction queue.
type Queue<'bump> = FileQueue<Transaction, &'bump Bump>;
/// The payments engine over the memory-mapped queue and file-backed state.
type Engine<'bump> = service::Engine<Queue<'bump>, MemoryState<'bump>>;

/// The engine state: per-client account state and the transaction lifecycle,
/// both held in memory-mapped, file-backed stores.
#[derive(Debug)]
struct MemoryState<'bump> {
    /// The store holding each client's account state.
    clients: Clients<'bump>,
    /// The store holding each transaction's lifecycle.
    records: Records<'bump>,
}

impl State for MemoryState<'_> {
    type ClientsError = ClientsError<EnumerateError, FetchError>;
    type LoadClientError = LoadClientError<FetchError>;
    type LoadTxError = LoadTxError<FetchError>;
    type StoreClientError = StoreClientError<WriteError>;
    type StoreTxError = StoreTxError<WriteError>;

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
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProcessError {
    /// Opening or sizing a backing file failed.
    #[error("opening or sizing a backing file failed: {0}")]
    Backing(#[from] BackingFileError),
    /// Opening a memory-mapped key-value store failed.
    #[error("opening a key-value store failed: {0}")]
    Kv(#[source] MapError),
    /// Opening the memory-mapped transaction queue failed.
    #[error("opening the transaction queue failed: {0}")]
    Queue(#[source] OpenError),
    /// Serving the transaction stream through the engine failed.
    #[error(transparent)]
    Serve(#[from] ServeError),
}

/// Failure while serving the transaction stream through the engine.
///
/// The service and transport layers parameterise their error types over the
/// concrete state, queue, and allocator; the CLI owns those concrete choices,
/// so it flattens them into one non-generic enum that names each layer's
/// concrete error without leaking the composition detail through the public
/// [`RunError`] surface.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServeError {
    /// Reading the next balance batch failed.
    #[error("reading the balance stream failed: {0}")]
    Balance(BalanceError),
    /// Draining the transaction stream into balances failed.
    #[error("consuming the transaction stream failed: {0}")]
    Consume(ConsumeError),
    /// Opening the standard output failed.
    #[error("opening the standard output failed: {0}")]
    Stdout(StdoutError),
    /// A serve error variant this CLI build does not recognise.
    #[error("unrecognised serve error")]
    Unrecognised,
    /// Writing a balance batch to stdout failed.
    #[error("writing the balance stream failed: {0}")]
    Write(WriteBatchError<StdoutSinkError>),
}

/// Failure while projecting a balance row from the engine state.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BalanceError {
    /// Enumerating the client records failed.
    #[error("enumerating the client records failed: {0}")]
    Clients(ClientsError<EnumerateError, FetchError>),
    /// Projecting a client's balance total overflowed.
    #[error(transparent)]
    Overflow(#[from] OverflowError),
    /// A balance error variant this CLI build does not recognise.
    #[error("unrecognised balance error")]
    Unrecognised,
}

/// Failure while draining the transaction stream into a balance stream.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConsumeError {
    /// Appending a transaction batch to the queue failed.
    #[error("appending a transaction batch failed: {0}")]
    Append(FileAppendError),
    /// Applying a transaction to the engine state failed.
    #[error(transparent)]
    Apply(#[from] ApplyError),
    /// Reading the next transaction batch failed.
    #[error("reading the next transaction batch failed: {0}")]
    NextBatch(FileNextBatchError),
    /// The queue offset overflowed.
    #[error("the queue offset overflowed")]
    OffsetOverflow,
    /// The batch length no longer fits the offset type.
    #[error("the batch length no longer fits the offset type: {0}")]
    OffsetWiden(#[from] core::num::TryFromIntError),
    /// Reading the transaction stream failed.
    #[error("reading the transaction stream failed: {0}")]
    Stream(ReadBatchError),
    /// A consume error variant this CLI build does not recognise.
    #[error("unrecognised consume error")]
    Unrecognised,
}

/// Failure to apply one transaction to the engine state.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApplyError {
    /// Loading a client's account state failed.
    #[error("loading the client state failed: {0}")]
    LoadClient(LoadClientError<FetchError>),
    /// Loading a transaction's lifecycle failed.
    #[error("loading the transaction state failed: {0}")]
    LoadTx(LoadTxError<FetchError>),
    /// A referenced transaction's client had no account state.
    #[error("client {client:?} has a transaction but no account state")]
    MissingAccount {
        /// The client whose account state is absent.
        client: ClientId,
    },
    /// Account arithmetic overflowed while applying a transaction.
    #[error("account arithmetic overflowed for client {client:?}")]
    Overflow {
        /// The client whose balance overflowed.
        client: ClientId,
    },
    /// Storing a client's account state failed.
    #[error("storing the client state failed: {0}")]
    StoreClient(StoreClientError<WriteError>),
    /// Storing a transaction's lifecycle failed.
    #[error("storing the transaction state failed: {0}")]
    StoreTx(StoreTxError<WriteError>),
    /// An apply error variant this CLI build does not recognise.
    #[error("unrecognised apply error")]
    Unrecognised,
    /// The transaction kind is not recognised by this engine version.
    #[error("unsupported transaction kind")]
    Unsupported,
}

impl<'bump> From<service::ApplyError<MemoryState<'bump>>> for ApplyError {
    #[inline]
    fn from(source: service::ApplyError<MemoryState<'bump>>) -> Self {
        match source {
            service::ApplyError::LoadClient(error) => Self::LoadClient(error),
            service::ApplyError::LoadTx(error) => Self::LoadTx(error),
            service::ApplyError::MissingAccount { client } => Self::MissingAccount { client },
            service::ApplyError::Overflow { client } => Self::Overflow { client },
            service::ApplyError::StoreClient(error) => Self::StoreClient(error),
            service::ApplyError::StoreTx(error) => Self::StoreTx(error),
            service::ApplyError::Unsupported => Self::Unsupported,
            _ => Self::Unrecognised,
        }
    }
}

impl<'bump> From<service::BalanceError<MemoryState<'bump>>> for BalanceError {
    #[inline]
    fn from(source: service::BalanceError<MemoryState<'bump>>) -> Self {
        match source {
            service::BalanceError::Clients(error) => Self::Clients(error),
            service::BalanceError::Overflow(error) => Self::Overflow(error),
            _ => Self::Unrecognised,
        }
    }
}

impl<'bump>
    From<
        service::ConsumeError<
            Queue<'bump>,
            MemoryState<'bump>,
            Transactions<compio::fs::Stdin, &'bump Bump>,
        >,
    > for ConsumeError
{
    #[inline]
    fn from(
        source: service::ConsumeError<
            Queue<'bump>,
            MemoryState<'bump>,
            Transactions<compio::fs::Stdin, &'bump Bump>,
        >,
    ) -> Self {
        match source {
            service::ConsumeError::Append(error) => Self::Append(error),
            service::ConsumeError::Apply(error) => Self::Apply(error.into()),
            service::ConsumeError::NextBatch(error) => Self::NextBatch(error),
            service::ConsumeError::OffsetOverflow => Self::OffsetOverflow,
            service::ConsumeError::OffsetWiden(error) => Self::OffsetWiden(error),
            service::ConsumeError::Stream(error) => Self::Stream(error),
            _ => Self::Unrecognised,
        }
    }
}

impl<'bump> From<transport::ServeError<Engine<'bump>, compio::fs::Stdin, &'bump Bump>>
    for ServeError
{
    #[inline]
    fn from(source: transport::ServeError<Engine<'bump>, compio::fs::Stdin, &'bump Bump>) -> Self {
        match source {
            transport::ServeError::Balance(error) => Self::Balance(error.into()),
            transport::ServeError::Consume(error) => Self::Consume(error.into()),
            transport::ServeError::Stdout(error) => Self::Stdout(error),
            transport::ServeError::Write(error) => Self::Write(error),
            _ => Self::Unrecognised,
        }
    }
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
    /// The backing-file name does not fit the fixed name buffer.
    #[error("the backing-file name does not fit the fixed name buffer")]
    Overflow,
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
) -> Result<(compio::fs::File, PathBuf), BackingFileError> {
    // Render the process-unique backing-file name `"{name}-{pid}"` into a fixed
    // stack buffer — no heap allocation and no owned-string conversion. `u32`
    // has at most ten decimal digits and every caller passes a short literal
    // `name`, so the fixed buffer is ample; a name that would not fit is
    // reported as `Overflow` rather than being silently truncated.
    let mut buf = [0_u8; 64];
    let name_len = name.len();
    let separator = name_len.checked_add(1).ok_or(BackingFileError::Overflow)?;
    let total = separator
        .checked_add(10)
        .ok_or(BackingFileError::Overflow)?;
    if total > buf.len() {
        return Err(BackingFileError::Overflow);
    }
    buf.get_mut(..name_len)
        .ok_or(BackingFileError::Overflow)?
        .copy_from_slice(name.as_bytes());
    *buf.get_mut(name_len).ok_or(BackingFileError::Overflow)? = b'-';

    // The decimal digits of the process id, least significant last.
    let mut digits = [0_u8; 10];
    let mut value = std::process::id();
    let mut digit_len = digits.len();
    loop {
        digit_len = digit_len.checked_sub(1).ok_or(BackingFileError::Overflow)?;
        let rem = value.checked_rem(10).ok_or(BackingFileError::Overflow)?;
        let digit = u8::try_from(rem).map_err(|_overflow| BackingFileError::Overflow)?;
        let byte = b'0'.checked_add(digit).ok_or(BackingFileError::Overflow)?;
        *digits
            .get_mut(digit_len)
            .ok_or(BackingFileError::Overflow)? = byte;
        value = value.checked_div(10).ok_or(BackingFileError::Overflow)?;
        if value == 0 {
            break;
        }
    }
    let digit_count = digits
        .len()
        .checked_sub(digit_len)
        .ok_or(BackingFileError::Overflow)?;
    let end = separator
        .checked_add(digit_count)
        .ok_or(BackingFileError::Overflow)?;
    buf.get_mut(separator..end)
        .ok_or(BackingFileError::Overflow)?
        .copy_from_slice(digits.get(digit_len..).ok_or(BackingFileError::Overflow)?);
    let component = std::ffi::OsStr::from_bytes(buf.get(..end).ok_or(BackingFileError::Overflow)?);
    let path = std::env::temp_dir().join(component);

    let file = compio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .await?;
    file.set_len(u64::try_from(capacity)?).await?;
    Ok((file, path))
}

/// Drive the engine over `source`, projecting the resulting balances to
/// stdout.
async fn process(
    source: transport::Input<compio::fs::Stdin>,
    token: &CancellationToken,
    tracker: &TaskTracker,
) -> Result<(), ProcessError> {
    let bump = Bump::with_capacity(ARENA_CAPACITY);
    let (clients_file, clients_path) =
        open_backing_file("record-processor-clients", KV_CAPACITY).await?;
    let clients: Clients<'_> = FileKv::try_from(FileKvParts {
        alloc: &bump,
        hasher: RandomState::new(),
        file: clients_file,
        path: clients_path,
        tracker: tracker.clone(),
        capacity: KV_CAPACITY,
    })
    .map_err(ProcessError::Kv)?;
    let (records_file, records_path) =
        open_backing_file("record-processor-records", KV_CAPACITY).await?;
    let records: Records<'_> = FileKv::try_from(FileKvParts {
        alloc: &bump,
        hasher: RandomState::new(),
        file: records_file,
        path: records_path,
        tracker: tracker.clone(),
        capacity: KV_CAPACITY,
    })
    .map_err(ProcessError::Kv)?;
    let (queue_file, queue_path) =
        open_backing_file("record-processor-queue", QUEUE_CAPACITY).await?;
    let queue: Queue<'_> = FileQueue::try_from(FileQueueParts {
        alloc: &bump,
        file: queue_file,
        path: queue_path,
        tracker: tracker.clone(),
        capacity: QUEUE_CAPACITY,
    })
    .map_err(ProcessError::Queue)?;
    let state = MemoryState { clients, records };
    let engine = Engine::from(EngineParts { queue, state });
    transport::serve::<_, _, &Bump>(engine, source, &bump, token)
        .await
        .map_err(ServeError::from)?;
    Ok(())
}

/// Open the input source and drive the engine.
async fn serve_input(
    input: Option<String>,
    token: &CancellationToken,
    tracker: &TaskTracker,
) -> Result<(), ServeInputError> {
    if let Some(path) = input {
        let file = compio::fs::File::open(path).await?;
        process(
            transport::Input::<compio::fs::Stdin>::from(file),
            token,
            tracker,
        )
        .await?;
    } else {
        let stdin = compio::fs::stdin();
        process(transport::Input::Stream(stdin), token, tracker).await?;
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
        let tracker = TaskTracker::new();
        let pipeline = std::pin::pin!(serve_input(input, &pipeline_token, &tracker));
        let cancel = std::pin::pin!(cancel_on_signal(token));
        let result = match futures_util::future::select(pipeline, cancel).await {
            Either::Left((result, _)) => result.map_err(RunError::Serve),
            Either::Right((cancel_result, remaining)) => {
                // Drain the pipeline to completion on the cancellation path so
                // its backends drop and schedule their cleanup before the
                // tracker is closed and awaited below.
                let served = remaining.await.map_err(RunError::Serve);
                match cancel_result {
                    Err(cancel_error) => Err(RunError::Cancel(cancel_error)),
                    Ok(()) => served,
                }
            }
        };
        let _closed: bool = tracker.close();
        tracker.wait().await;
        result
    })
}
