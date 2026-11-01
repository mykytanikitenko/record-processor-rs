#!/usr/bin/env fish
# Non-rudzio test linter for the record-processor Rust workspace.
#
# Enforces that every test is registered through rudzio. Two checks:
#   1. ast-grep structural rules (Rust source) flag any non-rudzio test
#      attribute (`#[test]`, `#[tokio::test]`, `#[compio::test]`,
#      `#[async_std::test]`, `#[rstest]`, `#[test_case]`, `#[bench]`) and any
#      `#[rudzio::test]` that sits outside a `#[rudzio::suite(...)]` module.
#   2. A repo-wide text scan flags the same non-rudzio test attributes in
#      files ast-grep cannot parse (markdown, nix, fish, toml, ...). The
#      lint's own rule/test fixtures under `scripts/lint/` are excluded
#      because they intentionally contain these attributes as ast-grep test
#      data. `#[cfg(test)]` / `#[cfg(any(test, rudzio_test))]` gates are not
#      flagged by either check.
#
# Usage (run from the repo root):
#   fish scripts/lint/non-rudzio.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for combined findings, one JSON object per
#   line. Defaults to `tmp/non-rudzio.ndjson`. The `just lint` orchestrator
#   (scripts/lint.fish) passes `tmp/non-rudzio.ndjson`.
#
# Exit status:
#   0 — rule self-tests passed and no non-rudzio tests were found.
#   1 — rule self-tests failed, ast-grep errored (bad config, unparseable
#       rules, etc.), or non-rudzio tests were found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
set -l config (path resolve "$script_dir/sgconfig.yml")

cd "$repo_root"

# Output path: CLI arg, else $NON_RUDZIO_LINT_OUTPUT, else a tmp/ default.
set -l out "$argv[1]"
if test -z "$out"
    set out "$NON_RUDZIO_LINT_OUTPUT"
end
if test -z "$out"
    set out "tmp/non-rudzio.ndjson"
end
mkdir -p (path dirname "$out")

# The shared `rules/` directory holds both the panic rules and the non-rudzio
# rules, so the ast-grep scan and self-test are scoped to these two rule ids to
# avoid cross-reporting the other sub-lint's findings.
set -l rule_filter '^(non-rudzio-test-attribute|rudzio-test-outside-suite)$'

# 1. Self-test the rules before scanning. A rule that fails its own tests is
#    broken and must fail loudly — never scan with a silently no-op rule set.
echo "ℹ non-rudzio-lint: running rule self-tests..."
if not ast-grep test --config "$config" --skip-snapshot-tests --filter "$rule_filter"
    echo "✗ non-rudzio-lint: rule self-tests failed — aborting" >&2
    exit 1
end

# 2. ast-grep structural scan of Rust source. ast-grep exits 0 with no findings
#    and 1 when it finds `severity: error` matches, so both are "the linter
#    ran". A higher exit code means ast-grep itself failed.
echo "ℹ non-rudzio-lint: scanning Rust source..."
set -l stderr_file (mktemp)
ast-grep scan --config "$config" --json=stream --filter "$rule_filter" . >"$out" 2>"$stderr_file"
set -l scan_status $status
if test $scan_status -gt 1
    echo "✗ non-rudzio-lint: ast-grep scan failed (exit $scan_status)" >&2
    cat "$stderr_file" >&2
    rm -f "$stderr_file"
    exit 1
end
rm -f "$stderr_file"

# 3. Repo-wide text scan for non-rudzio test attributes in non-Rust files.
#    `#[cfg(test)]`/`#[cfg(any(test, rudzio_test))]` gates are not matched
#    because the alternation only names test-registration attributes. Reads
#    index blobs (sparse-checkout safe) like the other sub-lints.
set -l attr_regex '#\[(test|tokio::test|compio::test|async_std::test|rstest|test_case|bench)\b'
set -l text_hits (git grep --cached -nE "$attr_regex" -- . ':(exclude)*.rs' ':(exclude)scripts/lint/**' 2>/dev/null)

# Convert `git grep` output (`path:line:text`) into ndjson findings and append
# to the ast-grep stream so both checks share one output file.
set -l text_json
for hit in $text_hits
    set -l parts (string split -m2 ':' -- "$hit")
    set -a text_json (jq -cn --arg f "$parts[1]" --argjson l "$parts[2]" \
        '{file:$f,line:$l,ruleId:"non-rudzio-test-attribute",message:"non-rudzio test attribute in a non-Rust file (config/scripts must register tests through rudzio)"}')
end
if test (count $text_json) -gt 0
    printf '%s\n' $text_json >>"$out"
end

# 4. Human-readable diagnostics + per-rule counts.
set -l total 0
if test -s "$out"
    set total (string trim (wc -l < "$out"))
end
if test $total -gt 0
    echo "✗ non-rudzio-lint: $total non-rudzio test finding(s):"
    echo
    jq -r 'if .range then "\(.file):\(.range.start.line + 1):\(.range.start.column + 1)  \(.ruleId)  \(.message)" else "\(.file):\(.line)  \(.ruleId)  \(.message)" end' "$out"
    echo
    echo "Per-rule counts:"
    jq -r '.ruleId' "$out" | sort | uniq -c | sort -rn
else
    echo "✓ non-rudzio-lint: no non-rudzio tests found"
end
echo "ℹ non-rudzio-lint: JSON stream → $out"

if test $total -gt 0
    echo "✗ non-rudzio-lint: non-rudzio tests found — failing lint" >&2
    exit 1
end
exit 0
