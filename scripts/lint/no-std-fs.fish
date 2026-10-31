#!/usr/bin/env fish
# no-std-fs linter for the record-processor Rust workspace.
#
# Forbids `std::fs` everywhere — `use std::fs…`, `std::fs::X` in types and
# expressions, and any fully-qualified `std::fs::*` path. There are NO
# permitted forms: every filesystem access must go through async `compio::fs`,
# so any `std::fs` token is a violation. No path/file exclusions either.
#
# Extraction is structural (ast-grep), then classified by jq
# (.jq/no-std-fs.jq), which renames the `std-fs-usage` rule to `no-std-fs` and
# shapes the finding records.
#
# Usage (run from the repo root):
#   fish scripts/lint/no-std-fs.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for findings, one JSON object per line.
#   Defaults to `tmp/no-std-fs.ndjson`. The `just lint` orchestrator
#   (scripts/lint.fish) passes `tmp/no-std-fs.ndjson`.
#
# Exit status:
#   0 — self-tests passed and no forbidden std::fs usage was found.
#   1 — self-tests failed, ast-grep/jq errored, or a forbidden usage was found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
set -g config (path resolve "$script_dir/no-std-fs/sgconfig.yml")
set -g filter "$repo_root/.jq/no-std-fs.jq"

cd "$repo_root"

set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/no-std-fs.ndjson"
end
mkdir -p (path dirname "$out")

# The jq classifier is a hard dependency: if absent, the sub-lint would produce
# no findings and falsely report PASS. Fail loudly instead.
if not test -f "$filter"
    echo "✗ no-std-fs: filter file missing: $filter" >&2
    exit 1
end

# Run the extraction + classification pipeline over one directory, writing the
# finding array to $outfile. Shared by the real scan and the runtime self-test.
function _run_pipeline --argument-names dir outfile
    set -l raw (mktemp)
    set -l stderr_file (mktemp)
    ast-grep scan --config "$config" --json=stream "$dir" >"$raw" 2>"$stderr_file"
    set -l scan_status $status
    if test $scan_status -gt 1
        echo "✗ no-std-fs: ast-grep scan failed (exit $scan_status)" >&2
        cat "$stderr_file" >&2
        rm -f "$raw" "$stderr_file"
        return 1
    end
    rm -f "$stderr_file"
    jq -s -f "$filter" "$raw" | jq -c '.[]' >"$outfile"
    or begin
        rm -f "$raw"
        return 1
    end
    rm -f "$raw"
    return 0
end

# Runtime self-test: generate fixtures covering every should-flag / should-pass
# case and assert the classifier's outcome on each.
function _self_test
    set -g selftest_dir "tmp/no-std-fs-selftest-$fish_pid"
    set -l dir "$selftest_dir"
    mkdir -p "$dir/flag" "$dir/pass"

    function _selftest_cleanup --on-event fish_exit
        rm -rf "$selftest_dir"
    end

    # Flag cases (should produce findings).
    printf '%s\n' \
        'use std::fs;' \
        'fn a() { let _ = std::fs::read_to_string("x"); }' >"$dir/flag/use.rs"
    printf '%s\n' \
        'use std::fs::{File, read};' \
        'struct R { f: std::fs::File }' \
        'fn b() -> std::fs::File { let _ = std::fs::write("x", &[]); todo!() }' >"$dir/flag/paths.rs"

    set -l flag_out (mktemp)
    _run_pipeline "$dir/flag" "$flag_out"
    or return 1
    set -l flag_total (string trim (wc -l < "$flag_out"))
    if test "$flag_total" -lt 5
        echo "✗ no-std-fs: self-test failed — expected >=5 forbidden findings, got $flag_total" >&2
        jq -c . "$flag_out" >&2
        return 1
    end
    rm -f "$flag_out"

    # Pass cases (should produce no findings).
    printf '%s\n' \
        'async fn w() { let _ = compio::fs::write(&p, b).await; }' >"$dir/pass/compio.rs"
    printf '%s\n' \
        'async fn r() { let _ = compio::fs::read(&p).await; }' >"$dir/pass/read.rs"
    printf '%s\n' \
        'fn clean(x: usize) -> usize { x }' >"$dir/pass/clean.rs"

    set -l pass_out (mktemp)
    _run_pipeline "$dir/pass" "$pass_out"
    or return 1
    set -l pass_total (string trim (wc -l < "$pass_out"))
    if test "$pass_total" -ne 0
        echo "✗ no-std-fs: self-test failed — permitted fixture flagged" >&2
        jq -c . "$pass_out" >&2
        return 1
    end
    rm -f "$pass_out"

    echo "✓ no-std-fs: self-tests passed"
    return 0
end

# 1. Self-test the extraction rules before scanning.
echo "ℹ no-std-fs: running rule self-tests..."
if not ast-grep test --config "$config" --skip-snapshot-tests
    echo "✗ no-std-fs: rule self-tests failed — aborting" >&2
    exit 1
end

# 2. Runtime self-test of the full extraction + classification pipeline.
echo "ℹ no-std-fs: running self-tests..."
if not _self_test
    exit 1
end

# 3. Scan the whole workspace and classify.
echo "ℹ no-std-fs: scanning workspace..."
set -l raw (mktemp)
set -l stderr_file (mktemp)
ast-grep scan --config "$config" --json=stream . >"$raw" 2>"$stderr_file"
set -l scan_status $status
if test $scan_status -gt 1
    echo "✗ no-std-fs: ast-grep scan failed (exit $scan_status)" >&2
    cat "$stderr_file" >&2
    rm -f "$raw" "$stderr_file"
    exit 1
end
rm -f "$stderr_file"
jq -s -f "$filter" "$raw" | jq -c '.[]' >"$out"
or begin
    echo "✗ no-std-fs: jq classification failed" >&2
    rm -f "$raw"
    exit 1
end
rm -f "$raw"

# 4. Human-readable diagnostics + summary.
set -l total (string trim (wc -l < "$out"))
if test "$total" -gt 0
    echo "✗ no-std-fs: $total forbidden std::fs usage(s):"
    echo
    jq -r '"\(.file):\(.line):\(.column)  no-std-fs  \(.message)"' "$out"
    echo
    echo "Per-file counts:"
    jq -r '.file' "$out" | sort | uniq -c | sort -rn
    echo
else
    echo "✓ no-std-fs: no forbidden std::fs usage"
end
echo "ℹ no-std-fs: JSON stream → $out"

if test "$total" -gt 0
    echo "✗ no-std-fs: forbidden std::fs usage found — failing lint" >&2
    exit 1
end
exit 0
