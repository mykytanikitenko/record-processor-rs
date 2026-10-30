#!/usr/bin/env fish
# no-tokio-runtime linter for the record-processor Rust workspace.
#
# compio is the project's only async runtime. The single permitted Tokio-family
# surface is `tokio_util`'s `CancellationToken` and `TaskTracker` (re-exported
# through `rudzio::tokio_util`), and even those must never be *spawned on* a
# Tokio runtime: background tasks go through `compio::runtime::spawn`, and the
# tracker is fed via `tracker.track_future(fut)`.
#
# Forbidden, in ALL code including tests:
#   * `tokio::spawn`, `tokio::task::spawn`, `tokio::task::spawn_local`,
#     `tokio::task::spawn_blocking`, `tokio::task::block_in_place`,
#     `tokio::task::LocalSet` — the spawn/task-in-place APIs.
#   * any path under `tokio::runtime::` (`Runtime`, `Builder`, `Handle`, …).
#   * the `#[tokio::main]` / `#[tokio::test]` attributes.
#   * the `tokio::time` / `tokio::net` / `tokio::fs` / `tokio::io` /
#     `tokio::process` / `tokio::signal` modules and the `tokio::select!` /
#     `tokio::join!` / `tokio::try_join!` macros.
#   * a `spawn` / `spawn_local` / `spawn_on` / `spawn_local_on` /
#     `spawn_blocking` / `spawn_blocking_on` call on a `TaskTracker` value.
#
# The first four categories collapse to a single structural rule
# (`tokio-path`): any `scoped_identifier` whose text begins with `tokio::`, or
# any `use` declaration mentioning `tokio::`. Because `tokio_util::…` is a
# distinct identifier (not `tokio::…`), the permitted `CancellationToken` /
# `TaskTracker` paths are never matched.
#
# The fifth category (`tokio-tracker-spawn`) needs the TaskTracker *type*, which
# ast-grep cannot see. Precision trade-off (documented): rather than the coarse
# "flag every `X.spawn*(…)` when the file mentions `TaskTracker`", the rule
# flags only (a) `TaskTracker::spawn*(…)`-qualified calls (any path depth) and
# (b) `X.spawn*(…)` where the receiver's final name is exactly `tracker` or
# `*_tracker` (`tracker.spawn(…)`, `self.tracker.spawn(…)`, `foo_tracker.spawn(…)`).
# This keeps `compio::process::Command::new(..)…spawn()` (receiver `cmd`, a
# method chain) and `compio::runtime::spawn(tracker.track_future(fut))`
# (`compio::`, not `tokio::`) un-flagged — verified by the self-tests and by the
# current tree (`tests/e2e/runner.rs`). A receiver bound to a non-`TaskTracker`
# that happens to be named `*_tracker` would be a false positive, but no such
# binding exists in the workspace and the convention is to name the tracker
# `tracker`.
#
# Usage (run from the repo root):
#   fish scripts/lint/no-tokio-runtime.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for `ast-grep --json=stream` findings, one JSON
#   object per line. Defaults to `tmp/no-tokio-runtime.ndjson`. The `just lint`
#   orchestrator (scripts/lint.fish) passes `tmp/no-tokio-runtime.ndjson`.
#
# Exit status:
#   0 — rule self-tests passed and no forbidden Tokio runtime usage was found.
#   1 — rule self-tests failed, ast-grep itself errored (bad config, unparseable
#       rules), or forbidden Tokio runtime usage was found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
set -l config (path resolve "$script_dir/no-tokio-runtime/sgconfig.yml")

# Scan from the repo root so ast-grep emits repo-relative paths and honors the
# repo's .gitignore (target/, tmp/, build dirs are skipped).
cd "$repo_root"

# Output path: CLI arg, else a hardcoded tmp/ default.
set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/no-tokio-runtime.ndjson"
end
mkdir -p (path dirname "$out")

# 1. Self-test the rules before scanning. A rule that fails its own tests is
#    broken and must fail loudly — never scan with a silently no-op rule set.
echo "ℹ no-tokio-runtime-lint: running rule self-tests..."
if not ast-grep test --config "$config" --skip-snapshot-tests
    echo "✗ no-tokio-runtime-lint: rule self-tests failed — aborting" >&2
    exit 1
end

# 2. ast-grep structural scan. ast-grep exits 0 with no findings and 1 when it
#    finds `severity: error` matches, so both are "the linter ran". A higher
#    exit code means ast-grep itself failed.
echo "ℹ no-tokio-runtime-lint: scanning workspace..."
set -l stderr_file (mktemp)
ast-grep scan --config "$config" --json=stream . >"$out" 2>"$stderr_file"
set -l scan_status $status
if test $scan_status -gt 1
    echo "✗ no-tokio-runtime-lint: ast-grep scan failed (exit $scan_status)" >&2
    cat "$stderr_file" >&2
    rm -f "$stderr_file"
    exit 1
end
rm -f "$stderr_file"

# 3. Human-readable diagnostics + per-rule counts. ast-grep reports lines and
#    columns 0-based, so bump both by one for editor-friendly 1-based
#    `file:line:col`.
set -l had_findings 0
if test -s "$out"
    set had_findings 1
    set -l total (string trim (wc -l < "$out"))
    echo "✗ no-tokio-runtime-lint: $total forbidden Tokio runtime usage(s):"
    echo
    jq -r '"\(.file):\(.range.start.line + 1):\(.range.start.column + 1)  \(.ruleId)  \(.message)"' "$out"
    echo
    echo "Per-rule counts:"
    jq -r '.ruleId' "$out" | sort | uniq -c | sort -rn
else
    echo "✓ no-tokio-runtime-lint: no forbidden Tokio runtime usage"
end
echo "ℹ no-tokio-runtime-lint: JSON stream → $out"
if test $had_findings -eq 1
    echo "✗ no-tokio-runtime-lint: forbidden Tokio runtime usage found — failing lint" >&2
    exit 1
end
exit 0
