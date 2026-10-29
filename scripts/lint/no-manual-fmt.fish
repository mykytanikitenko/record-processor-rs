#!/usr/bin/env fish
# no-manual-fmt linter for the record-processor Rust workspace.
#
# Forbids hand-written implementations of the standard-library fmt traits —
# `Display`, `Debug`, `LowerHex`, `UpperHex`, `Binary`, `Octal`, `Pointer`,
# `LowerExp`, `UpperExp`, and `Write` — whether written with a bare trait name
# (`impl Debug for …`) or a path (`impl core::fmt::Debug for …`). Formatting
# must come from a derive (`#[derive(Debug)]`, `derive_more::Display`, …), so
# the printed representation stays mechanical and cannot drift from the fields.
#
# Scope: every git-tracked `.rs` file — crates/, src/, and the test targets
# under crates/*/tests/. The rule carries no `files:` filter, so nothing is
# excluded.
#
# Derive attributes are fine: `#[derive(Debug)]` produces no `impl_item` node,
# so only hand-written `impl … for …` blocks are matched.
#
# Usage (run from the repo root):
#   fish scripts/lint/no-manual-fmt.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for `ast-grep --json=stream` findings, one
#   JSON object per line. Defaults to `tmp/no-manual-fmt.ndjson`. The `just
#   lint` orchestrator (scripts/lint.fish) passes `tmp/no-manual-fmt.ndjson`.
#
# Exit status:
#   0 — rule self-tests passed and no hand-written fmt trait impl was found.
#   1 — rule self-tests failed, ast-grep itself errored, or a hand-written fmt
#       trait impl was found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
set -l config (path resolve "$script_dir/no-manual-fmt/sgconfig.yml")

# Scan from the repo root so ast-grep emits repo-relative paths and honors the
# repo's .gitignore (target/, tmp/, build dirs are skipped).
cd "$repo_root"

set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/no-manual-fmt.ndjson"
end
mkdir -p (path dirname "$out")

# 1. Self-test the rule before scanning. A rule that fails its own tests is
#    broken and must fail loudly — never scan with a silently no-op rule set.
echo "ℹ no-manual-fmt-lint: running rule self-tests..."
if not ast-grep test --config "$config" --skip-snapshot-tests
    echo "✗ no-manual-fmt-lint: rule self-tests failed — aborting" >&2
    exit 1
end

# 2. ast-grep structural scan. ast-grep exits 0 with no findings and 1 when it
#    finds `severity: error` matches, so both are "the linter ran". A higher
#    exit code means ast-grep itself failed.
echo "ℹ no-manual-fmt-lint: scanning workspace..."
set -l stderr_file (mktemp)
ast-grep scan --config "$config" --json=stream . >"$out" 2>"$stderr_file"
set -l scan_status $status
if test $scan_status -gt 1
    echo "✗ no-manual-fmt-lint: ast-grep scan failed (exit $scan_status)" >&2
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
    echo "✗ no-manual-fmt-lint: $total hand-written fmt trait impl(s):"
    echo
    jq -r '"\(.file):\(.range.start.line + 1):\(.range.start.column + 1)  \(.ruleId)  \(.message)"' "$out"
    echo
    echo "Per-rule counts:"
    jq -r '.ruleId' "$out" | sort | uniq -c | sort -rn
else
    echo "✓ no-manual-fmt-lint: no hand-written fmt trait impls found"
end
echo "ℹ no-manual-fmt-lint: JSON stream → $out"
if test $had_findings -eq 1
    echo "✗ no-manual-fmt-lint: hand-written fmt trait impls found — failing lint" >&2
    exit 1
end
exit 0
