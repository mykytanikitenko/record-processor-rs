//! End-to-end test harness for the `record-processor` binary.
//!
//! Wiring-only entry point. Implementation lives in the sibling modules and
//! every test is registered in [`crate::cases`]. `#[rudzio::main]` discovers
//! and runs the suites registered across this crate.

mod case_loader;
mod cases;
mod comparator;
mod runner;

#[rudzio::main]
fn main() {}
