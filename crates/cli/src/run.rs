use crate::error::Error;

/// Parses the command line and runs the payments engine.
///
/// # Errors
///
/// Returns [`Error::Args`] when the arguments are malformed and
/// [`Error::MissingInput`] when the input path is absent.
#[inline]
pub fn run() -> Result<(), Error> {
    let mut args = pico_args::Arguments::from_env();
    let input: Option<String> = args.opt_free_from_str()?;
    if input.is_none() {
        return Err(Error::MissingInput);
    }
    Ok(())
}
