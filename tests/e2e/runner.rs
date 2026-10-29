//! Binary spawning and process-output capture for the e2e harness.
//!
//! Process spawning goes through `compio::process::Command`, the project's
//! only allowed process layer. Each helper starts a fresh `compio` runtime and
//! blocks on the process operation: the workspace's `compio` (0.19.2) is a
//! different crate version from the `compio` runtime rudzio's `runtime-compio`
//! feature pulls in (0.11), so the process calls cannot run on rudzio's suite
//! runtime and instead own a short-lived workspace-compio runtime per call.
//!
//! Note that compio's process API surfaces `std::process::Stdio` and
//! `std::process::Output` in its public signatures (there is no compio-native
//! `Stdio` or `Output` re-export), so those two `std::process` types are
//! referenced here rather than `std::process::Command`.

use anyhow::Context as _;
use compio::io::AsyncWriteExt as _;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

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

/// Resolve the compiled `record-processor` binary path.
///
/// Uses rudzio's [`bin!`](rudzio::bin) macro so the path resolves correctly
/// across cargo test layouts (integration tests, `--lib`, shared runner).
pub(crate) fn resolve_bin() -> PathBuf {
    rudzio::bin!("record-processor")
}

/// Run the binary at `bin` against `input` and capture stdout, stderr, and
/// exit status.
///
/// # Errors
///
/// Returns an error when the compio runtime cannot start, the binary cannot be
/// spawned, or its output is not valid UTF-8.
pub(crate) fn run_binary(bin: &Path, input: &Path) -> anyhow::Result<ProcessOutput> {
    let rt = compio::runtime::Runtime::new().context("failed to start the compio runtime")?;
    rt.block_on(async move {
        let output = compio::process::Command::new(bin)
            .arg(input)
            .stdout(Stdio::piped())?
            .stderr(Stdio::piped())?
            .output()
            .await
            .context("failed to run the record-processor binary")?;
        process_output(output)
    })
}

/// Run the binary at `bin` with no arguments, feeding `input` to its standard
/// input, and capture stdout, stderr, and exit status.
///
/// # Errors
///
/// Returns an error when the compio runtime cannot start, the binary cannot be
/// spawned, its standard input cannot be written, or its output is not valid
/// UTF-8.
pub(crate) fn run_binary_stdin(bin: &Path, input: String) -> anyhow::Result<ProcessOutput> {
    let rt = compio::runtime::Runtime::new().context("failed to start the compio runtime")?;
    rt.block_on(async move {
        let mut cmd = compio::process::Command::new(bin);
        let mut child = cmd
            .stdin(Stdio::piped())?
            .stdout(Stdio::piped())?
            .stderr(Stdio::piped())?
            .spawn()
            .context("failed to spawn the record-processor binary")?;
        let mut stdin = child
            .stdin
            .take()
            .context("record-processor stdin is not piped")?;
        let write_result = stdin.write_all(input).await.0;
        write_result.context("failed to write record-processor stdin")?;
        drop(stdin);
        let output = child
            .wait_with_output()
            .await
            .context("failed to wait for the record-processor binary")?;
        process_output(output)
    })
}

/// Convert a spawned `Output` into a decoded [`ProcessOutput`].
fn process_output(output: Output) -> anyhow::Result<ProcessOutput> {
    let stdout = String::from_utf8(output.stdout).context("binary stdout is not UTF-8")?;
    let stderr = String::from_utf8(output.stderr).context("binary stderr is not UTF-8")?;
    Ok(ProcessOutput {
        status_code: output.status.code(),
        status_success: output.status.success(),
        stderr,
        stdout,
    })
}
