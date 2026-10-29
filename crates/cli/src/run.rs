//! Command-line wiring: parse arguments, open the input, and drive the
//! payments engine to stdout.

use service::Service as _;

/// Failure while running the command-line entry point.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RunError {
    /// The command line could not be parsed.
    #[error("invalid arguments: {0}")]
    Args(#[from] pico_args::Error),
    /// The compio runtime could not be started.
    #[error("runtime failed: {0}")]
    Runtime(#[from] std::io::Error),
    /// Serving the input through the engine failed.
    #[error("processing failed: {0}")]
    Serve(#[from] ServeInputError),
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
    /// The engine failed while consuming the transaction stream.
    #[error(transparent)]
    Serve(#[from] service::ServeError<transport::ReaderError>),
    /// Writing the account balances failed.
    #[error(transparent)]
    Write(#[from] transport::WriterError),
}

/// Drive the engine over `source`, writing balances to stdout.
async fn process<S>(source: S) -> Result<(), ProcessError>
where
    S: compio::io::AsyncRead,
{
    let reader = transport::TransactionReader::from(source);
    let balances = service::Engine::from(()).serve(reader).await?;
    let mut writer = transport::AccountBalanceWriter::from(compio::fs::stdout());
    writer.write_all(&balances).await?;
    Ok(())
}

/// Open the input source and drive the engine.
async fn serve_input(input: Option<String>) -> Result<(), ServeInputError> {
    if let Some(path) = input {
        let file = compio::fs::File::open(path).await?;
        process(std::io::Cursor::new(file)).await?;
    } else {
        process(compio::fs::stdin()).await?;
    }
    Ok(())
}

/// Parses the command line and runs the payments engine.
///
/// The sole optional free argument is the input CSV path; when absent, the
/// transaction stream is read from standard input.
///
/// # Errors
///
/// Returns [`RunError::Args`] when the arguments are malformed,
/// [`RunError::Runtime`] when the runtime cannot start, and
/// [`RunError::Serve`] when the engine pipeline fails.
#[inline]
pub fn run() -> Result<(), RunError> {
    let mut args = pico_args::Arguments::from_env();
    let input: Option<String> = args.opt_free_from_str()?;
    let runtime = compio::runtime::Runtime::new()?;
    runtime.block_on(serve_input(input))?;
    Ok(())
}
