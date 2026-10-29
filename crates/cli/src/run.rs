/// Failure while running the command-line entry point.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RunError {
    /// The command line could not be parsed.
    #[error("invalid arguments: {0}")]
    Args(#[from] pico_args::Error),
    /// The input CSV path argument was not supplied.
    #[error("usage: record-processor <transactions.csv>")]
    MissingInput,
}

/// Parses the command line and runs the payments engine.
///
/// # Errors
///
/// Returns [`RunError::Args`] when the arguments are malformed and
/// [`RunError::MissingInput`] when the input path is absent.
#[inline]
pub fn run() -> Result<(), RunError> {
    let mut args = pico_args::Arguments::from_env();
    let input: Option<String> = args.opt_free_from_str()?;
    if input.is_none() {
        return Err(RunError::MissingInput);
    }
    Ok(())
}
