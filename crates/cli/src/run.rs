//! Command-line wiring: parse arguments, build the runtime, and drive the
//! payments engine to stdout with cooperative cancellation.

use allocator_api2::alloc::Global;
use allocator_api2::boxed::Box;
use futures_util::future::Either;
use service::engine_state::KvState;
use service::{ClientId, EngineParts, ServiceState, State, Transaction, TxId, TxRecord};
use std::hash::RandomState;
use storage::memory_kv::{KeysError, LoadError, MemoryKv, Parts as KvParts, StoreError};
use storage::memory_queue::{MemoryQueue, Parts as QueueParts};
use tokio_util::sync::CancellationToken;

/// The in-memory store of each client's account state.
type Clients = MemoryKv<ClientId, ServiceState, RandomState, Global>;
/// The in-memory store of each transaction's lifecycle.
type Records = MemoryKv<TxId, TxRecord, RandomState, Global>;
/// The in-memory transaction queue.
type Queue = MemoryQueue<Transaction, Global>;
/// The payments engine over the in-memory storage.
type Engine = service::Engine<Queue, MemoryState>;

/// The engine state: per-client account state and the transaction lifecycle,
/// both held in memory.
#[derive(Debug)]
struct MemoryState {
    /// The store holding each client's account state.
    clients: Clients,
    /// The store holding each transaction's lifecycle.
    records: Records,
}

impl State for MemoryState {
    type ClientsError = service::ClientsError<KeysError, LoadError>;
    type LoadClientError = service::LoadClientError<LoadError>;
    type LoadTxError = service::LoadTxError<LoadError>;
    type StoreClientError = service::StoreClientError<StoreError>;
    type StoreTxError = service::StoreTxError<StoreError>;

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
    /// Serving the transaction stream through the engine failed.
    #[error("serving the transaction stream failed: {0}")]
    Serve(#[source] Box<dyn std::error::Error + Send + Sync>),
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

/// Drive the engine over `source`, projecting the resulting balances to
/// stdout.
async fn process(
    source: transport::Input<compio::fs::Stdin>,
    token: &CancellationToken,
) -> Result<(), ProcessError> {
    let clients: Clients = MemoryKv::from(KvParts {
        alloc: Global,
        hasher: RandomState::new(),
    });
    let records: Records = MemoryKv::from(KvParts {
        alloc: Global,
        hasher: RandomState::new(),
    });
    let queue: Queue = MemoryQueue::from(QueueParts { alloc: Global });
    let state = MemoryState { clients, records };
    let engine = Engine::from(EngineParts { queue, state });
    transport::serve(engine, source, token)
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
