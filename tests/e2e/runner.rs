//! Binary spawning and process-output capture for the e2e harness.

use anyhow::Context as _;
use std::path::Path;

/// Captured output of one `record-processor` invocation.
#[derive(Debug)]
pub(crate) struct ProcessOutput {
    /// The raw exit code, when the platform reports one.
    pub(crate) status_code: Option<i32>,
    /// Whether the process exited successfully.
    pub(crate) status_success: bool,
    /// Standard error, decoded as UTF-8.
    pub(crate) stderr: String,
    /// Standard output, decoded as UTF-8.
    pub(crate) stdout: String,
}

/// Run the binary against `input` and capture stdout, stderr, and exit status.
///
/// # Errors
///
/// Returns an error when the binary cannot be spawned or its output is not
/// valid UTF-8.
pub(crate) fn run_binary(input: &Path) -> anyhow::Result<ProcessOutput> {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_record-processor"))
        .arg(input)
        .output()
        .context("failed to spawn the record-processor binary")?;
    let stdout = String::from_utf8(output.stdout).context("binary stdout is not UTF-8")?;
    let stderr = String::from_utf8(output.stderr).context("binary stderr is not UTF-8")?;
    Ok(ProcessOutput {
        status_code: output.status.code(),
        status_success: output.status.success(),
        stderr,
        stdout,
    })
}
