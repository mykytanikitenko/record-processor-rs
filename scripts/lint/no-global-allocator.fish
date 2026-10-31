#!/usr/bin/env fish
# no-global-allocator linter for the record-processor Rust workspace.
#
# Forbids naming the concrete `Global` allocator in library crates (storage,
# service, transport, domain) and forbids passing allocators by reference.
# Library crates must thread `A: Allocator` generics and take allocators by
# value; only the CLI composition root may name a concrete allocator. Test
# code (`tests/`) is exempt.
#
# Extraction is structural (ast-grep), then classified by jq
# (.jq/no-global-allocator.jq), which scopes the `global-allocator` and
# `allocator-by-ref` rules to library-crate source and renames them to
# `no-global-allocator`.
#
# Usage (run from the repo root):
#   fish scripts/lint/no-global-allocator.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for findings, one JSON object per line.
#   Defaults to `tmp/no-global-allocator.ndjson`. The `just lint` orchestrator
#   (scripts/lint.fish) passes `tmp/no-global-allocator.ndjson`.
#
# Exit status:
#   0 — self-tests passed and no forbidden allocator usage was found.
#   1 — self-tests failed, ast-grep/jq errored, or a usage was found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
set -g config (path resolve "$script_dir/no-global-allocator/sgconfig.yml")
set -g filter "$repo_root/.jq/no-global-allocator.jq"

cd "$repo_root"

set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/no-global-allocator.ndjson"
end
mkdir -p (path dirname "$out")

if not test -f "$filter"
    echo "✗ no-global-allocator: filter file missing: $filter" >&2
    exit 1
end

function _run_pipeline --argument-names dir outfile
    set -l raw (mktemp)
    set -l stderr_file (mktemp)
    ast-grep scan --config "$config" --json=stream "$dir" >"$raw" 2>"$stderr_file"
    set -l scan_status $status
    if test $scan_status -gt 1
        echo "✗ no-global-allocator: ast-grep scan failed (exit $scan_status)" >&2
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

function _self_test
    set -g selftest_dir "tmp/no-global-allocator-selftest-$fish_pid"
    set -l dir "$selftest_dir"
    mkdir -p "$dir/flag/crates/storage/src" \
        "$dir/pass/crates/cli/src" \
        "$dir/pass/crates/storage/tests" \
        "$dir/pass/crates/storage/src"

    function _selftest_cleanup --on-event fish_exit
        rm -rf "$selftest_dir"
    end

    # Flag cases: concrete Global + by-reference allocator in a library crate.
    printf '%s\n' \
        'use allocator_api2::alloc::Global;' \
        'fn f(alloc: &impl Allocator) {}' >"$dir/flag/crates/storage/src/global.rs"

    set -l flag_out (mktemp)
    _run_pipeline "$dir/flag" "$flag_out"
    or return 1
    set -l flag_total (string trim (wc -l < "$flag_out"))
    if test "$flag_total" -lt 2
        echo "✗ no-global-allocator: self-test failed — expected >=2 findings, got $flag_total" >&2
        jq -c . "$flag_out" >&2
        return 1
    end
    rm -f "$flag_out"

    # Pass cases: Global in the CLI composition root, Global in test code, and
    # a by-value allocator in a library crate.
    printf '%s\n' \
        'use allocator_api2::alloc::Global;' \
        'fn main() { let _ = Global; }' >"$dir/pass/crates/cli/src/run.rs"
    printf '%s\n' \
        'use allocator_api2::alloc::Global;' \
        '#[test]' \
        'fn it() { let _ = Global; }' >"$dir/pass/crates/storage/tests/it.rs"
    printf '%s\n' \
        'fn f<A: Allocator>(alloc: A) {}' >"$dir/pass/crates/storage/src/clean.rs"

    set -l pass_out (mktemp)
    _run_pipeline "$dir/pass" "$pass_out"
    or return 1
    set -l pass_total (string trim (wc -l < "$pass_out"))
    if test "$pass_total" -ne 0
        echo "✗ no-global-allocator: self-test failed — permitted fixture flagged" >&2
        jq -c . "$pass_out" >&2
        return 1
    end
    rm -f "$pass_out"

    echo "✓ no-global-allocator: self-tests passed"
    return 0
end

echo "ℹ no-global-allocator: running rule self-tests..."
if not ast-grep test --config "$config" --skip-snapshot-tests
    echo "✗ no-global-allocator: rule self-tests failed — aborting" >&2
    exit 1
end

echo "ℹ no-global-allocator: running self-tests..."
if not _self_test
    exit 1
end

echo "ℹ no-global-allocator: scanning workspace..."
set -l raw (mktemp)
set -l stderr_file (mktemp)
ast-grep scan --config "$config" --json=stream . >"$raw" 2>"$stderr_file"
set -l scan_status $status
if test $scan_status -gt 1
    echo "✗ no-global-allocator: ast-grep scan failed (exit $scan_status)" >&2
    cat "$stderr_file" >&2
    rm -f "$raw" "$stderr_file"
    exit 1
end
rm -f "$stderr_file"
jq -s -f "$filter" "$raw" | jq -c '.[]' >"$out"
or begin
    echo "✗ no-global-allocator: jq classification failed" >&2
    rm -f "$raw"
    exit 1
end
rm -f "$raw"

set -l total (string trim (wc -l < "$out"))
if test "$total" -gt 0
    echo "✗ no-global-allocator: $total forbidden allocator usage(s):"
    echo
    jq -r '"\(.file):\(.line):\(.column)  no-global-allocator  \(.message)"' "$out"
    echo
    echo "Per-file counts:"
    jq -r '.file' "$out" | sort | uniq -c | sort -rn
    echo
else
    echo "✓ no-global-allocator: no forbidden allocator usage"
end
echo "ℹ no-global-allocator: JSON stream → $out"

if test "$total" -gt 0
    echo "✗ no-global-allocator: forbidden allocator usage found — failing lint" >&2
    exit 1
end
exit 0
