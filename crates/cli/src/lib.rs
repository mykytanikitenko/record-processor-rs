//! Command-line wiring from CSV transport through the payments engine to stdout.

/// Error type returned by the entry point.
mod error;
/// Entry point parsing arguments and driving the engine.
mod run;

pub use error::Error;
pub use run::run;
