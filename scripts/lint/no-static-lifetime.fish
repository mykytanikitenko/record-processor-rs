#!/usr/bin/env fish
# no-static-lifetime linter for the record-processor Rust workspace.
#
# Forbids the `'static` lifetime token anywhere in Rust source: type positions
# (`&'static str`), trait bounds (`T: 'static`, `dyn X + 'static`), `where`
# clauses (`where T: 'static`), and type arguments (`Cow<'static, str>`). A
# `'static` lifetime pins data to the program lifetime and hides its real
# owner; a named lifetime (or an owned value) keeps the borrow tied to the
# data's actual owner.
#
# Scope: every git-tracked `.rs` file — crates/, src/, and the test targets
# under crates/*/tests/. The rule carries no `files:` filter, so nothing is
# excluded.
#
# Not covered: `static` ITEMS (`static FOO: u8 = 1;`) are a distinct
# tree-sitter node kind (`static_item`), not a `lifetime`, and are therefore
# not matched by this rule. Global-state policy is a separate concern; this
# lint governs lifetimes only.
#
# Usage (run from the repo root):
#   fish scripts/lint/no-static-lifetime.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for `ast-grep --json=stream` findings, one
#   JSON object per line. Defaults to `tmp/no-static-lifetime.ndjson`. The
#   `just lint` orchestrator (scripts/lint.fish) passes
#   `tmp/no-static-lifetime.ndjson`.
#
# Exit status:
#   0 — rule self-tests passed and no `'static` lifetimes were found.
#   1 — rule self-tests failed, ast-grep itself errored (bad config,
#       unparseable rules), or `'static` lifetimes were found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
set -l config (path resolve "$script_dir/no-static-lifetime/sgconfig.yml")

# Scan from the repo root so ast-grep emits repo-relative paths and honors the
# repo's .gitignore (target/, tmp/, build dirs are skipped).
cd "$repo_root"

# Output path: CLI arg, else a hardcoded tmp/ default.
set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/no-static-lifetime.ndjson"
end
mkdir -p (path dirname "$out")

# 1. Self-test the rule before scanning. A rule that fails its own tests is
#    broken and must fail loudly — never scan with a silently no-op rule set.
echo "ℹ no-static-lifetime-lint: running rule self-tests..."
if not ast-grep test --config "$config" --skip-snapshot-tests
    echo "✗ no-static-lifetime-lint: rule self-tests failed — aborting" >&2
    exit 1
end

# 2. ast-grep structural scan. ast-grep exits 0 with no findings and 1 when it
#    finds `severity: error` matches, so both are "the linter ran". A higher
#    exit code means ast-grep itself failed.
echo "ℹ no-static-lifetime-lint: scanning workspace..."
set -l stderr_file (mktemp)
ast-grep scan --config "$config" --json=stream . >"$out" 2>"$stderr_file"
set -l scan_status $status
if test $scan_status -gt 1
    echo "✗ no-static-lifetime-lint: ast-grep scan failed (exit $scan_status)" >&2
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
    echo "✗ no-static-lifetime-lint: $total 'static lifetime violation(s):"
    echo
    jq -r '"\(.file):\(.range.start.line + 1):\(.range.start.column + 1)  \(.ruleId)  \(.message)"' "$out"
    echo
    echo "Per-rule counts:"
    jq -r '.ruleId' "$out" | sort | uniq -c | sort -rn
else
    echo "✓ no-static-lifetime-lint: no 'static lifetimes found"
end
echo "ℹ no-static-lifetime-lint: JSON stream → $out"
if test $had_findings -eq 1
    echo "✗ no-static-lifetime-lint: 'static lifetimes found — failing lint" >&2
    exit 1
end
exit 0
