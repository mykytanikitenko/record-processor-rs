/// Failure while running the command-line entry point.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The command line could not be parsed.
    #[error("invalid arguments: {0}")]
    Args(#[from] pico_args::Error),
    /// The input CSV path argument was not supplied.
    #[error("usage: record-processor <transactions.csv>")]
    MissingInput,
}
