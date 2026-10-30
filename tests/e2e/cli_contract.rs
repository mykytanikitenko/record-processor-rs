//! Binary-level CLI-contract suite.
//!
//! Unlike the fixture-driven cases in [`crate::cases`], these tests drive the
//! compiled `record-processor` binary directly to pin the CLI's I/O contract:
//! the input source (file path versus standard input), the failure shape on a
//! missing or malformed input, and the header-only success path.
//!
//! The suite owns a hand-rolled [`rudzio::context::Suite`] / [`rudzio::context::Test`]
//! pair so the compiled binary path is resolved exactly once, in suite setup,
//! and threaded to every test through the per-test context. The suite runs on
//! rudzio's `futures` runtime because the workspace's `compio` (0.19.2) is a
//! different crate version from the `compio` runtime rudzio's `runtime-compio`
//! feature pulls in (0.11); each process call therefore starts its own
//! short-lived workspace-compio runtime inside [`crate::runner`].
//!
//! Signal cancellation (SIGINT/SIGTERM while the binary waits on standard
//! input) is NOT wired yet. `cli::run` starts the compio runtime and blocks on
//! the input source with no signal handler and no cancellation token, so the
//! operating system's default disposition terminates the process and no
//! cancellation exit code or stderr contract exists to assert here. Add a
//! cancellation test once the CLI grows one.

use std::convert::Infallible;
use std::path::{Path, PathBuf};

use rudzio::Config;
use rudzio::context;
use rudzio::runtime::Runtime;
use rudzio::tokio_util::sync::CancellationToken;
use rudzio::tokio_util::task::TaskTracker;

use crate::case_loader::load_case;
use crate::runner::ProcessOutput;

/// Exact stdout the binary must emit for the spec's worked example.
const SPEC_EXAMPLE_STDOUT: &str =
    "client,available,held,total,locked\n1,1.5,0,1.5,false\n2,2,0,2,false\n";

/// Exact stdout the binary must emit for a header-only input.
const HEADER_ONLY_STDOUT: &str = "client,available,held,total,locked\n";

/// Suite context: the compiled binary path, resolved once in setup.
#[derive(derive_more::Debug)]
struct CliSuite<'suite_context, R>
where
    R: Runtime<'suite_context> + Sync,
{
    /// Resolved `record-processor` binary path, shared by every test.
    #[debug(skip)]
    bin: PathBuf,
    /// Root cancellation token whose children are handed out to per-test contexts.
    #[debug(skip)]
    cancel: CancellationToken,
    /// Borrow of the async runtime driving the suite context.
    #[debug(skip)]
    rt: &'suite_context R,
    /// Tracks spawned background tasks so teardown can wait for them.
    #[debug(skip)]
    tracker: TaskTracker,
}

/// Per-test context: borrows the suite's binary path and task tracker.
#[derive(derive_more::Debug)]
struct CliTest<'test_context, R>
where
    R: Runtime<'test_context> + Sync,
{
    /// The suite's resolved binary path, borrowed for this test.
    #[debug(skip)]
    bin: &'test_context Path,
    /// Child cancellation token scoped to this test.
    #[debug(skip)]
    cancel: CancellationToken,
    /// Resolved CLI/env configuration, handed down from the suite.
    #[debug(skip)]
    config: &'test_context Config,
    /// Borrow of the async runtime driving the test.
    #[debug(skip)]
    rt: &'test_context R,
    /// Suite-shared task tracker, borrowed rather than cloned.
    #[debug(skip)]
    tracker: &'test_context TaskTracker,
}

impl<'suite_context, R> context::Suite<'suite_context, R> for CliSuite<'suite_context, R>
where
    R: for<'rt> Runtime<'rt> + Sync,
{
    type ContextError = Infallible;
    type SetupError = Infallible;
    type TeardownError = Infallible;
    type Test<'test_context>
        = CliTest<'test_context, R>
    where
        Self: 'test_context;

    #[inline]
    fn cancel_token(&self) -> &CancellationToken {
        &self.cancel
    }

    #[inline]
    async fn context<'test_context>(
        &'test_context self,
        cancel: CancellationToken,
        config: &'test_context Config,
    ) -> Result<CliTest<'test_context, R>, Infallible> {
        Ok(CliTest {
            bin: self.bin.as_path(),
            cancel,
            config,
            rt: self.rt,
            tracker: &self.tracker,
        })
    }

    #[inline]
    fn rt(&self) -> &'suite_context R {
        self.rt
    }

    #[inline]
    async fn setup(
        rt: &'suite_context R,
        cancel: CancellationToken,
        _config: &'suite_context Config,
    ) -> Result<Self, Infallible> {
        let bin = crate::runner::resolve_bin();
        Ok(Self {
            bin,
            cancel: cancel.child_token(),
            rt,
            tracker: TaskTracker::new(),
        })
    }

    #[inline]
    async fn teardown(self, _cancel: CancellationToken) -> Result<(), Infallible> {
        self.cancel.cancel();
        let _closed: bool = self.tracker.close();
        self.tracker.wait().await;
        Ok(())
    }

    #[inline]
    fn tracker(&self) -> &TaskTracker {
        &self.tracker
    }
}

impl<'test_context, R> context::Test<'test_context, R> for CliTest<'test_context, R>
where
    R: Runtime<'test_context> + Sync,
{
    type TeardownError = Infallible;

    #[inline]
    fn cancel_token(&self) -> &CancellationToken {
        &self.cancel
    }

    #[inline]
    fn config(&self) -> &Config {
        self.config
    }

    #[inline]
    fn rt(&self) -> &'test_context R {
        self.rt
    }

    #[inline]
    async fn teardown(self, _cancel: CancellationToken) -> Result<(), Infallible> {
        self.cancel.cancel();
        Ok(())
    }

    #[inline]
    fn tracker(&self) -> &TaskTracker {
        self.tracker
    }
}

/// Assert a successful run: zero exit and empty stderr.
fn assert_success(name: &str, out: &ProcessOutput) -> anyhow::Result<()> {
    anyhow::ensure!(
        out.status_success,
        "{name}: expected exit 0, got {:?}",
        out.status_code
    );
    anyhow::ensure!(
        out.stderr.is_empty(),
        "{name}: expected empty stderr, got:\n{}",
        out.stderr
    );
    Ok(())
}

/// Assert a failed run: non-zero exit, empty stdout, non-empty stderr.
fn assert_failure(name: &str, out: &ProcessOutput) -> anyhow::Result<()> {
    anyhow::ensure!(
        !out.status_success,
        "{name}: expected a non-zero exit, got {:?}",
        out.status_code
    );
    anyhow::ensure!(
        out.stdout.is_empty(),
        "{name}: expected empty stdout, got:\n{}",
        out.stdout
    );
    anyhow::ensure!(
        !out.stderr.is_empty(),
        "{name}: expected non-empty stderr, got empty"
    );
    Ok(())
}

#[rudzio::suite([(runtime = rudzio::runtime::futures::ThreadPool::new, suite = CliSuite, test = CliTest)])]
mod tests {
    use std::path::Path;

    use anyhow::Context as _;

    use super::{CliTest, HEADER_ONLY_STDOUT, SPEC_EXAMPLE_STDOUT, assert_failure, assert_success};
    use crate::runner::{run_binary, run_binary_stdin};

    #[rudzio::test]
    fn file_argument_and_stdin_emit_identical_output(ctx: &CliTest) -> anyhow::Result<()> {
        let case = super::load_case("001-spec-example")?;
        let input = std::fs::read_to_string(&case.input)
            .with_context(|| format!("failed to read {}", case.input.display()))?;

        let from_file = run_binary(ctx.bin, &case.input)?;
        assert_success("file argument", &from_file)?;
        anyhow::ensure!(
            from_file.stdout == SPEC_EXAMPLE_STDOUT,
            "file argument: unexpected stdout:\n{}",
            from_file.stdout
        );

        let from_stdin = run_binary_stdin(ctx.bin, input)?;
        assert_success("stdin", &from_stdin)?;
        anyhow::ensure!(
            from_stdin.stdout == SPEC_EXAMPLE_STDOUT,
            "stdin: unexpected stdout:\n{}",
            from_stdin.stdout
        );
        Ok(())
    }

    #[rudzio::test]
    fn missing_file_fails_cleanly(ctx: &CliTest) -> anyhow::Result<()> {
        let out = run_binary(
            ctx.bin,
            Path::new("/nonexistent/record-processor-input.csv"),
        )?;
        assert_failure("missing file", &out)?;
        anyhow::ensure!(
            out.stderr.starts_with("Error:"),
            "missing file: stderr must be the process error, got:\n{}",
            out.stderr
        );
        Ok(())
    }

    #[rudzio::test]
    fn malformed_row_fails_cleanly(ctx: &CliTest) -> anyhow::Result<()> {
        let case = super::load_case("061-malformed-unknown-type")?;
        let out = run_binary(ctx.bin, &case.input)?;
        assert_failure("malformed row", &out)?;
        anyhow::ensure!(
            out.stderr.starts_with("Error:"),
            "malformed row: stderr must be the process error, got:\n{}",
            out.stderr
        );
        Ok(())
    }

    #[rudzio::test]
    fn header_only_input_emits_header_only(ctx: &CliTest) -> anyhow::Result<()> {
        let case = super::load_case("060-header-only")?;
        let out = run_binary(ctx.bin, &case.input)?;
        assert_success("header-only", &out)?;
        anyhow::ensure!(
            out.stdout == HEADER_ONLY_STDOUT,
            "header-only: unexpected stdout:\n{}",
            out.stdout
        );
        Ok(())
    }

    /// Current behaviour: the CLI reads the single free argument verbatim, so
    /// an unknown option such as `--help` is handed to `File::open` as a path
    /// and fails with a not-found error. Argument validation is not wired yet.
    #[rudzio::test]
    fn unknown_option_is_treated_as_a_path(ctx: &CliTest) -> anyhow::Result<()> {
        let out = run_binary(ctx.bin, Path::new("--help"))?;
        assert_failure("unknown option", &out)?;
        Ok(())
    }
}
