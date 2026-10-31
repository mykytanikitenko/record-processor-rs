//! Command-line wiring: parse arguments, build the runtime, and drive the
//! payments engine to stdout with cooperative cancellation.

use compio::io::AsyncRead;
use futures_util::future::Either;
use tokio_util::sync::CancellationToken;

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
    /// The payments engine has no implementation over the storage contracts.
    #[error("the payments engine is not implemented")]
    Unimplemented,
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

/// Drive the engine over `source`; the engine awaits its storage-backed
/// implementation, so every source reports [`ProcessError::Unimplemented`].
async fn process<S>(
    _source: transport::Input<S>,
    _token: &CancellationToken,
) -> Result<(), ProcessError>
where
    S: AsyncRead,
{
    Err(ProcessError::Unimplemented)
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
