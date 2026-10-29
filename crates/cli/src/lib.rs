//! Command-line wiring from CSV transport through the payments engine to stdout.

/// Entry point parsing arguments and driving the engine.
mod run;

pub use run::{RunError, run};
