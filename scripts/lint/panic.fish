#!/usr/bin/env fish
# Panic-pattern linter for the record-processor Rust workspace.
#
# Structural matching via ast-grep (never regex over source text), so
# `#[attr]` attributes, array types `[T; N]`, array literals `[1, 2]`,
# comments and string literals are never flagged as indexing or macro use.
#
# Usage (run from the repo root):
#   fish scripts/lint/panic.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for ast-grep `--json=stream` findings, one
#   JSON object per line. Defaults to `tmp/panic.ndjson`. The `just lint`
#   orchestrator (scripts/lint.fish) passes `tmp/panic.ndjson`.
#
# Exit status:
#   0 — rule self-tests passed and no panic patterns were found.
#   1 — rule self-tests failed, ast-grep itself errored (bad config,
#       unparseable rules, etc.), or panic patterns were found. Findings are
#       failures: the workspace is not panic-free until the count is zero.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
set -l config (path resolve "$script_dir/sgconfig.yml")

# The shared `rules/` directory also holds the non-rudzio rules
# (`non-rudzio-test-attribute`, `rudzio-test-outside-suite`). Scope the scan
# and self-test to the panic rules only so this sub-lint does not cross-report
# the non-rudzio sub-lint's findings.
set -l panic_filter '^(test-returns-anyhow-result|assert-macros|panic-macros|index-panics|pretty-assertions|should-panic-attribute|unwrap-methods)$'

# Scan from the repo root so ast-grep emits repo-relative paths and honors the
# repo's .gitignore (target/, tmp/, build dirs are skipped).
cd "$repo_root"

# Output path: CLI arg, else $PANIC_LINT_OUTPUT, else a tmp/ default.
set -l out "$argv[1]"
if test -z "$out"
    set out "$PANIC_LINT_OUTPUT"
end
if test -z "$out"
    set out "tmp/panic.ndjson"
end
mkdir -p (path dirname "$out")

# 1. Self-test the rules before scanning. A rule that fails its own tests is
#    broken and must fail loudly — never scan with a silently no-op rule set.
echo "ℹ panic-lint: running rule self-tests..."
if not ast-grep test --config "$config" --skip-snapshot-tests --filter "$panic_filter"
    echo "✗ panic-lint: rule self-tests failed — aborting" >&2
    exit 1
end

# 2. Scan the whole workspace. ast-grep exits 0 with no findings and 1 when
#    it finds `severity: error` matches (exactly what we detect), so both are
#    "the linter ran". A higher exit code means ast-grep itself failed.
echo "ℹ panic-lint: scanning workspace..."
set -l stderr_file (mktemp)
ast-grep scan --config "$config" --json=stream --filter "$panic_filter" . >"$out" 2>"$stderr_file"
set -l scan_status $status
if test $scan_status -gt 1
    echo "✗ panic-lint: ast-grep scan failed (exit $scan_status)" >&2
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
    echo "✗ panic-lint: $total panic pattern(s) found:"
    echo
    jq -r '"\(.file):\(.range.start.line + 1):\(.range.start.column + 1)  \(.ruleId)  \(.message)"' "$out"
    echo
    echo "Per-rule counts:"
    jq -r '.ruleId' "$out" | sort | uniq -c | sort -rn
else
    echo "✓ panic-lint: no panic patterns found"
end
echo "ℹ panic-lint: JSON stream → $out"
if test $had_findings -eq 1
    echo "✗ panic-lint: panic patterns found — failing lint" >&2
    exit 1
end
exit 0
