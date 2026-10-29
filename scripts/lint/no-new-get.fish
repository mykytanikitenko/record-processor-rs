#!/usr/bin/env fish
# no-new-get linter for the record-processor Rust workspace.
#
# Enforces two construction/access conventions:
#   * `no-new` — forbids any function named `new` (free function, inherent
#     method, or trait method declaration/implementation). Construction is a
#     single conventional entry point: `TryFrom` when fallible, `From` when
#     infallible.
#   * `no-get` — forbids any function named `get` or `get_*` (getter
#     functions). Newtype contents are exposed read-only through
#     `derive_more::Deref` instead of hand-written getters.
#
# Scope: every git-tracked `.rs` file — crates/, src/, and the test targets
# under crates/*/tests/. Test code is NOT exempt: tests exercise the same
# public API (`TryFrom`/`From`/`Deref`) that production callers use, so a
# `new`/`get` in test code signals the same missing constructor/getter. The
# rules carry no `files:` filter, so nothing is excluded.
#
# Only DEFINITIONS are matched: calls into third-party constructors such as
# `Vec::new()` or `Foo::get()` are call/method-call nodes, not
# `function_item`/`function_signature_item` nodes, and are left untouched.
#
# Usage (run from the repo root):
#   fish scripts/lint/no-new-get.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for `ast-grep --json=stream` findings, one
#   JSON object per line. Defaults to `tmp/no-new-get.ndjson`. The `just
#   lint` orchestrator (scripts/lint.fish) passes `tmp/no-new-get.ndjson`.
#
# Exit status:
#   0 — rule self-tests passed and no `new`/`get` definitions were found.
#   1 — rule self-tests failed, ast-grep itself errored (bad config,
#       unparseable rules), or `new`/`get` definitions were found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
set -l config (path resolve "$script_dir/no-new-get/sgconfig.yml")

# Scan from the repo root so ast-grep emits repo-relative paths and honors the
# repo's .gitignore (target/, tmp/, build dirs are skipped).
cd "$repo_root"

# Output path: CLI arg, else a hardcoded tmp/ default.
set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/no-new-get.ndjson"
end
mkdir -p (path dirname "$out")

# 1. Self-test the rules before scanning. A rule that fails its own tests is
#    broken and must fail loudly — never scan with a silently no-op rule set.
echo "ℹ no-new-get-lint: running rule self-tests..."
if not ast-grep test --config "$config" --skip-snapshot-tests
    echo "✗ no-new-get-lint: rule self-tests failed — aborting" >&2
    exit 1
end

# 2. ast-grep structural scan. ast-grep exits 0 with no findings and 1 when it
#    finds `severity: error` matches, so both are "the linter ran". A higher
#    exit code means ast-grep itself failed.
echo "ℹ no-new-get-lint: scanning workspace..."
set -l stderr_file (mktemp)
ast-grep scan --config "$config" --json=stream . >"$out" 2>"$stderr_file"
set -l scan_status $status
if test $scan_status -gt 1
    echo "✗ no-new-get-lint: ast-grep scan failed (exit $scan_status)" >&2
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
    echo "✗ no-new-get-lint: $total definition violation(s):"
    echo
    jq -r '"\(.file):\(.range.start.line + 1):\(.range.start.column + 1)  \(.ruleId)  \(.message)"' "$out"
    echo
    echo "Per-rule counts:"
    jq -r '.ruleId' "$out" | sort | uniq -c | sort -rn
else
    echo "✓ no-new-get-lint: no new/get definitions found"
end
echo "ℹ no-new-get-lint: JSON stream → $out"
if test $had_findings -eq 1
    echo "✗ no-new-get-lint: new/get definitions found — failing lint" >&2
    exit 1
end
exit 0
