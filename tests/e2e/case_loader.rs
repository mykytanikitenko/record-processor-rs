//! Fixture discovery and `case.toml` metadata loading for the e2e harness.

use anyhow::Context as _;
use std::path::{Path, PathBuf};

/// The decision the harness makes about a fixture case.
#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Outcome {
    /// Run the binary and require a non-zero exit with empty stdout.
    Error,
    /// The spec leaves this case undecided; the test fails loudly.
    PendingDecision,
    /// Run the binary and compare stdout to `expected.csv`.
    Success,
}

/// Metadata the harness needs from a fixture's `case.toml`.
///
/// `description` and `derivation` also appear in each `case.toml` but are
/// documentation only, so they are not loaded here.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct CaseMeta {
    /// Short id of the open decision (pending-decision only).
    pub(crate) decision: Option<String>,
    /// How the harness should treat the case.
    pub(crate) outcome: Outcome,
    /// Precise question the spec leaves open (pending-decision only).
    pub(crate) question: Option<String>,
}

/// A fully loaded fixture: its metadata and the paths to its data files.
#[derive(Debug)]
pub(crate) struct LoadedCase {
    /// Path to `expected.csv`, present only for success cases.
    pub(crate) expected: Option<PathBuf>,
    /// Path to the required `input.csv`.
    pub(crate) input: PathBuf,
    /// Parsed `case.toml` metadata.
    pub(crate) meta: CaseMeta,
}

/// The root directory holding every numbered fixture subdirectory.
fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/e2e/fixtures")
}

/// Load and validate the fixture named `name`.
///
/// # Errors
///
/// Returns an error when `case.toml` is missing or malformed, when a required
/// data file is missing, or when a pending-decision case lacks its decision id
/// or question.
pub(crate) fn load_case(name: &str) -> anyhow::Result<LoadedCase> {
    let dir = fixtures_root().join(name);
    let meta_path = dir.join("case.toml");
    let meta_toml = std::fs::read_to_string(&meta_path)
        .with_context(|| format!("case `{name}` is missing case.toml"))?;
    let meta: CaseMeta = toml::from_str(&meta_toml)
        .with_context(|| format!("case `{name}` has malformed case.toml"))?;

    let input = dir.join("input.csv");
    ensure_file(name, &input)?;

    let expected = match &meta.outcome {
        Outcome::Success => {
            let path = dir.join("expected.csv");
            ensure_file(name, &path)?;
            Some(path)
        }
        Outcome::Error | Outcome::PendingDecision => None,
    };

    if meta.outcome == Outcome::PendingDecision {
        anyhow::ensure!(
            meta.decision.is_some(),
            "case `{name}` is pending-decision but has no `decision` id"
        );
        anyhow::ensure!(
            meta.question.is_some(),
            "case `{name}` is pending-decision but has no `question`"
        );
    }

    Ok(LoadedCase {
        expected,
        input,
        meta,
    })
}

/// Ensure a fixture file exists, else fail loudly.
fn ensure_file(name: &str, path: &Path) -> anyhow::Result<()> {
    if path.is_file() {
        return Ok(());
    }
    anyhow::bail!("case `{name}` is missing {}", path.display())
}
