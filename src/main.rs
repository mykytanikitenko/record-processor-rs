//! Thin binary entry point for the record-processor payments engine.

/// Failure while mapping the command-line run onto a process exit status.
#[derive(Debug, thiserror::Error)]
enum MainError {
    /// The command-line run failed.
    #[error(transparent)]
    Run(#[from] cli::RunError),
}

fn main() -> Result<(), MainError> {
    cli::run()?;
    Ok(())
}
