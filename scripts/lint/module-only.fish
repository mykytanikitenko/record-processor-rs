#!/usr/bin/env fish
# module-only linter for the record-processor Rust workspace.
#
# Enforces that every `lib.rs` and `mod.rs` in the workspace holds module
# wiring only: `mod` declarations without a body, `use`/`pub use` re-exports,
# inner/outer attributes, doc and plain comments, and `extern crate`
# declarations. Any top-level implementation item — fn, struct, enum, union,
# impl, trait, const, static, type alias, `macro_rules!`, item-level macro
# invocation, inline `mod` body, or extern block — fails the lint.
#
# The ast-grep rule (scripts/lint/module-only/rules/module-only.yml) is an
# allow-list scoped to `**/lib.rs` and `**/mod.rs`: it matches every top-level
# item kind tree-sitter-rust can produce and rejects the ones outside the
# wiring set, so a newly introduced item kind fails by default.
#
# Usage (run from the repo root):
#   fish scripts/lint/module-only.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for `ast-grep --json=stream` findings, one
#   JSON object per line. Defaults to `tmp/module-only.ndjson`. The `just
#   lint` orchestrator (scripts/lint.fish) passes `tmp/module-only.ndjson`.
#
# Exit status:
#   0 — rule self-tests passed and no wiring violations were found.
#   1 — rule self-tests failed, ast-grep itself errored (bad config,
#       unparseable rules), or wiring violations were found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
set -l config (path resolve "$script_dir/module-only/sgconfig.yml")

# Scan from the repo root so ast-grep emits repo-relative paths and honors the
# repo's .gitignore (target/, tmp/, build dirs are skipped).
cd "$repo_root"

# Output path: CLI arg, else a hardcoded tmp/ default.
set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/module-only.ndjson"
end
mkdir -p (path dirname "$out")

# 1. Self-test the rule before scanning. A rule that fails its own tests is
#    broken and must fail loudly — never scan with a silently no-op rule set.
echo "ℹ module-only-lint: running rule self-tests..."
if not ast-grep test --config "$config" --skip-snapshot-tests
    echo "✗ module-only-lint: rule self-tests failed — aborting" >&2
    exit 1
end

# 2. ast-grep structural scan. The rule's `files:` glob limits it to
#    `**/lib.rs` and `**/mod.rs`, so the whole workspace is scanned and every
#    other file is ignored. ast-grep exits 0 with no findings and 1 when it
#    finds `severity: error` matches, so both are "the linter ran". A higher
#    exit code means ast-grep itself failed.
echo "ℹ module-only-lint: scanning workspace..."
set -l stderr_file (mktemp)
ast-grep scan --config "$config" --json=stream . >"$out" 2>"$stderr_file"
set -l scan_status $status
if test $scan_status -gt 1
    echo "✗ module-only-lint: ast-grep scan failed (exit $scan_status)" >&2
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
    echo "✗ module-only-lint: $total wiring violation(s):"
    echo
    jq -r '"\(.file):\(.range.start.line + 1):\(.range.start.column + 1)  \(.ruleId)  \(.message)"' "$out"
    echo
    echo "Per-rule counts:"
    jq -r '.ruleId' "$out" | sort | uniq -c | sort -rn
else
    echo "✓ module-only-lint: no wiring violations found"
end
echo "ℹ module-only-lint: JSON stream → $out"
if test $had_findings -eq 1
    echo "✗ module-only-lint: wiring violations found — failing lint" >&2
    exit 1
end
exit 0
