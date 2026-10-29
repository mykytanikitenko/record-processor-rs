#!/usr/bin/env fish
# no-fallbacks linter for the record-processor Rust workspace.
#
# Two detection channels, both failing (exit 1) on any finding:
#   1. ast-grep structural rules (scripts/lint/no-fallbacks/rules/) flag
#      silent fallbacks, error discards, default construction, unchecked
#      arithmetic, cloning, and error-source loss in Rust source under
#      crates/ and src/ (tests included).
#   2. A repo-wide text scan flags TODO/FIXME/XXX/HACK/TBD markers in tracked
#      non-Rust sources (fish, yml, toml, nix, Justfile, md). The lint's own
#      rule/test fixtures under scripts/lint/ are excluded because they
#      define the marker pattern themselves.
#
# Usage (run from the repo root):
#   fish scripts/lint/no-fallbacks.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for combined findings, one JSON object per
#   line. Defaults to `tmp/no-fallbacks.ndjson`. The `just lint` orchestrator
#   (scripts/lint.fish) passes `tmp/no-fallbacks.ndjson`.
#
# Exit status:
#   0 — rule self-tests passed and no fallback patterns were found.
#   1 — rule self-tests failed, ast-grep errored (bad config, unparseable
#       rules), or fallback patterns were found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
set -l config (path resolve "$script_dir/no-fallbacks/sgconfig.yml")

cd "$repo_root"

# Output path: CLI arg, else a hardcoded tmp/ default.
set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/no-fallbacks.ndjson"
end
mkdir -p (path dirname "$out")

# 1. Self-test the rules before scanning. A rule that fails its own tests is
#    broken and must fail loudly — never scan with a silently no-op rule set.
echo "ℹ no-fallbacks-lint: running rule self-tests..."
if not ast-grep test --config "$config" --skip-snapshot-tests
    echo "✗ no-fallbacks-lint: rule self-tests failed — aborting" >&2
    exit 1
end

# 2. ast-grep structural scan of Rust source. ast-grep exits 0 with no
#    findings and 1 when it finds `severity: error` matches, so both are "the
#    linter ran". A higher exit code means ast-grep itself failed.
echo "ℹ no-fallbacks-lint: scanning Rust source..."
set -l stderr_file (mktemp)
ast-grep scan --config "$config" --json=stream . >"$out" 2>"$stderr_file"
set -l scan_status $status
if test $scan_status -gt 1
    echo "✗ no-fallbacks-lint: ast-grep scan failed (exit $scan_status)" >&2
    cat "$stderr_file" >&2
    rm -f "$stderr_file"
    exit 1
end
rm -f "$stderr_file"

# 3. Repo-wide text scan for TODO/FIXME/XXX/HACK/TBD markers in non-Rust
#    files. `*.rs` files are handled structurally by the todo-comments rule;
#    the lint's own rule/test fixtures under scripts/lint/ are excluded. Reads
#    the index (sparse-checkout safe) like the other sub-lints.
set -l todo_regex '\b(TODO|FIXME|XXX|HACK|TBD)\b'
set -l text_hits (git grep -nIiE "$todo_regex" -- . ':!*.rs' ':!scripts/lint/**' ':!scripts/lint.fish' 2>/dev/null)

# Convert `git grep` output (`path:line:text`) into ndjson findings and append
# to the ast-grep stream so both checks share one output file.
set -l text_json
for hit in $text_hits
    set -l parts (string split -m2 ':' -- "$hit")
    set -a text_json (jq -cn --arg f "$parts[1]" --argjson l "$parts[2]" \
        '{file:$f,line:$l,ruleId:"todo-comments",message:"unresolved marker in a non-Rust source; resolve it or remove the comment"}')
end
if test (count $text_json) -gt 0
    printf '%s\n' $text_json >>"$out"
end

# 4. Human-readable diagnostics + per-rule counts. ast-grep reports lines and
#    columns 0-based, so bump by one for editor-friendly 1-based
#    `file:line:col`.
set -l total 0
if test -s "$out"
    set total (string trim (wc -l < "$out"))
end
if test $total -gt 0
    echo "✗ no-fallbacks-lint: $total fallback finding(s):"
    echo
    jq -r 'if .range then "\(.file):\(.range.start.line + 1):\(.range.start.column + 1)  \(.ruleId)  \(.message)" else "\(.file):\(.line)  \(.ruleId)  \(.message)" end' "$out"
    echo
    echo "Per-rule counts:"
    jq -r '.ruleId' "$out" | sort | uniq -c | sort -rn
else
    echo "✓ no-fallbacks-lint: no fallback patterns found"
end
echo "ℹ no-fallbacks-lint: JSON stream → $out"

if test $total -gt 0
    echo "✗ no-fallbacks-lint: fallback patterns found — failing lint" >&2
    exit 1
end
exit 0
