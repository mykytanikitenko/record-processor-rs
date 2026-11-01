#!/usr/bin/env fish
# no-std-io linter for the record-processor Rust workspace.
#
# Forbids `std::io` and `compio::io::compat::SyncStream` / `SyncStream` usage
# everywhere — `use std::io…`, `std::io::X` in types and expressions, bare
# `io::X` reached through `use std::io`, and the compio sync-stream adapter.
#
# Exactly two structural exceptions exist, both forced by compio's codec
# protocol and neither excludable by path/file (there are NO path/file
# exclusions — the permission is purely structural):
#   - a `#[from] std::io::Error` field inside a thiserror enum variant, and
#   - a `std::io::Result<…>` return type on a fn inside an `impl … Framer … for
#     …` block.
#
# Extraction is structural (ast-grep), then classified by jq
# (.jq/no-std-io.jq): a `std-io-usage` finding whose byte range lies inside a
# permitted finding's byte range is dropped.
#
# Usage (run from the repo root):
#   fish scripts/lint/no-std-io.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for findings, one JSON object per line.
#   Defaults to `tmp/no-std-io.ndjson`. The `just lint` orchestrator
#   (scripts/lint.fish) passes `tmp/no-std-io.ndjson`.
#
# Exit status:
#   0 — self-tests passed and no forbidden std::io / SyncStream usage was found.
#   1 — self-tests failed, ast-grep/jq errored, or a forbidden usage was found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
set -g config (path resolve "$script_dir/no-std-io/sgconfig.yml")
set -g filter "$repo_root/.jq/no-std-io.jq"

cd "$repo_root"

set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/no-std-io.ndjson"
end
mkdir -p (path dirname "$out")

# The jq classifier is a hard dependency: if absent, the sub-lint would produce
# no findings and falsely report PASS. Fail loudly instead.
if not test -f "$filter"
    echo "✗ no-std-io: filter file missing: $filter" >&2
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
        echo "✗ no-std-io: ast-grep scan failed (exit $scan_status)" >&2
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
    set -g selftest_dir "tmp/no-std-io-selftest-$fish_pid"
    set -l dir "$selftest_dir"
    mkdir -p "$dir/flag" "$dir/pass"

    function _selftest_cleanup --on-event fish_exit
        rm -rf "$selftest_dir"
    end

    # Flag cases (should produce findings).
    printf '%s\n' \
        'use std::io;' \
        'fn a() { let _ = io::Error::new(io::ErrorKind::Other, "x"); }' >"$dir/flag/use.rs"
    printf '%s\n' \
        'use std::io::{ErrorKind, Read as _};' \
        'use compio::io::compat::SyncStream;' \
        'struct R { source: SyncStream<u8> }' \
        'fn b() -> std::io::Result<()> { let _c = std::io::Cursor::new(&[]); Ok(()) }' >"$dir/flag/paths.rs"

    set -l flag_out (mktemp)
    _run_pipeline "$dir/flag" "$flag_out"
    or return 1
    set -l flag_total (string trim (wc -l < "$flag_out"))
    if test "$flag_total" -lt 5
        echo "✗ no-std-io: self-test failed — expected >=5 forbidden findings, got $flag_total" >&2
        jq -c . "$flag_out" >&2
        return 1
    end
    rm -f "$flag_out"

    # Pass cases (should produce no findings).
    printf '%s\n' \
        '#[derive(Debug, thiserror::Error)]' \
        'enum ReaderError {' \
        '    #[error("io")]' \
        '    Io(#[from] std::io::Error),' \
        '}' >"$dir/pass/from.rs"
    printf '%s\n' \
        'trait Framer<T> {}' \
        'struct R;' \
        'impl Framer<u8> for R {' \
        '    fn extract(&self) -> std::io::Result<Vec<u8>> { todo!() }' \
        '}' >"$dir/pass/framer.rs"
    printf '%s\n' \
        'fn clean(x: usize) -> usize { x }' >"$dir/pass/clean.rs"

    set -l pass_out (mktemp)
    _run_pipeline "$dir/pass" "$pass_out"
    or return 1
    set -l pass_total (string trim (wc -l < "$pass_out"))
    if test "$pass_total" -ne 0
        echo "✗ no-std-io: self-test failed — permitted fixture flagged" >&2
        jq -c . "$pass_out" >&2
        return 1
    end
    rm -f "$pass_out"

    echo "✓ no-std-io: self-tests passed"
    return 0
end

# 1. Self-test the extraction rules before scanning.
echo "ℹ no-std-io: running rule self-tests..."
if not ast-grep test --config "$config" --skip-snapshot-tests
    echo "✗ no-std-io: rule self-tests failed — aborting" >&2
    exit 1
end

# 2. Runtime self-test of the full extraction + classification pipeline.
echo "ℹ no-std-io: running self-tests..."
if not _self_test
    exit 1
end

# 3. Scan the whole workspace and classify.
echo "ℹ no-std-io: scanning workspace..."
set -l raw (mktemp)
set -l stderr_file (mktemp)
ast-grep scan --config "$config" --json=stream . >"$raw" 2>"$stderr_file"
set -l scan_status $status
if test $scan_status -gt 1
    echo "✗ no-std-io: ast-grep scan failed (exit $scan_status)" >&2
    cat "$stderr_file" >&2
    rm -f "$raw" "$stderr_file"
    exit 1
end
rm -f "$stderr_file"
jq -s -f "$filter" "$raw" | jq -c '.[]' >"$out"
or begin
    echo "✗ no-std-io: jq classification failed" >&2
    rm -f "$raw"
    exit 1
end
rm -f "$raw"

# 4. Human-readable diagnostics + summary.
set -l total (string trim (wc -l < "$out"))
if test "$total" -gt 0
    echo "✗ no-std-io: $total forbidden std::io / SyncStream usage(s):"
    echo
    jq -r '"\(.file):\(.line):\(.column)  no-std-io  \(.message)"' "$out"
    echo
    echo "Per-file counts:"
    jq -r '.file' "$out" | sort | uniq -c | sort -rn
    echo
else
    echo "✓ no-std-io: no forbidden std::io / SyncStream usage"
end
echo "ℹ no-std-io: JSON stream → $out"

if test "$total" -gt 0
    echo "✗ no-std-io: forbidden std::io / SyncStream usage found — failing lint" >&2
    exit 1
end
exit 0
