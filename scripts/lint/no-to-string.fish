#!/usr/bin/env fish
# no-to-string linter for the record-processor Rust workspace.
#
# Forbids owned-string conversion via `.to_string()` and `.to_owned()`
# (extending the no-cloning intent). The record-processor pipeline is
# zero-copy, so every owned-string conversion must be replaced by a
# `format!`, `String::from`, a `&str` borrow, or `Into::into`.
#
# Extraction is structural (ast-grep), then classified by jq
# (.jq/no-to-string.jq), which renames the `to-string-usage` rule to
# `no-to-string` and shapes the finding records.
#
# Usage (run from the repo root):
#   fish scripts/lint/no-to-string.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for findings, one JSON object per line.
#   Defaults to `tmp/no-to-string.ndjson`. The `just lint` orchestrator
#   (scripts/lint.fish) passes `tmp/no-to-string.ndjson`.
#
# Exit status:
#   0 — self-tests passed and no owned-string conversion was found.
#   1 — self-tests failed, ast-grep/jq errored, or a conversion was found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
set -g config (path resolve "$script_dir/no-to-string/sgconfig.yml")
set -g filter "$repo_root/.jq/no-to-string.jq"

cd "$repo_root"

set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/no-to-string.ndjson"
end
mkdir -p (path dirname "$out")

if not test -f "$filter"
    echo "✗ no-to-string: filter file missing: $filter" >&2
    exit 1
end

function _run_pipeline --argument-names dir outfile
    set -l raw (mktemp)
    set -l stderr_file (mktemp)
    ast-grep scan --config "$config" --json=stream "$dir" >"$raw" 2>"$stderr_file"
    set -l scan_status $status
    if test $scan_status -gt 1
        echo "✗ no-to-string: ast-grep scan failed (exit $scan_status)" >&2
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
    set -g selftest_dir "tmp/no-to-string-selftest-$fish_pid"
    set -l dir "$selftest_dir"
    mkdir -p "$dir/flag" "$dir/pass"

    function _selftest_cleanup --on-event fish_exit
        rm -rf "$selftest_dir"
    end

    # Flag cases (should produce findings).
    printf '%s\n' \
        'fn a(x: u32) { let _ = x.to_string(); }' \
        'fn b(s: &str) { let _ = s.to_owned(); }' >"$dir/flag/owned.rs"

    set -l flag_out (mktemp)
    _run_pipeline "$dir/flag" "$flag_out"
    or return 1
    set -l flag_total (string trim (wc -l < "$flag_out"))
    if test "$flag_total" -lt 2
        echo "✗ no-to-string: self-test failed — expected >=2 findings, got $flag_total" >&2
        jq -c . "$flag_out" >&2
        return 1
    end
    rm -f "$flag_out"

    # Pass cases (should produce no findings).
    printf '%s\n' \
        'fn a(x: u32) { let _ = format!("{x}"); }' \
        'fn b(s: &str) { let _ = String::from(s); }' \
        'fn c(s: &str) -> &str { s }' \
        'fn d(x: u32) { let t: String = x.into(); }' >"$dir/pass/clean.rs"

    set -l pass_out (mktemp)
    _run_pipeline "$dir/pass" "$pass_out"
    or return 1
    set -l pass_total (string trim (wc -l < "$pass_out"))
    if test "$pass_total" -ne 0
        echo "✗ no-to-string: self-test failed — permitted fixture flagged" >&2
        jq -c . "$pass_out" >&2
        return 1
    end
    rm -f "$pass_out"

    echo "✓ no-to-string: self-tests passed"
    return 0
end

echo "ℹ no-to-string: running rule self-tests..."
if not ast-grep test --config "$config" --skip-snapshot-tests
    echo "✗ no-to-string: rule self-tests failed — aborting" >&2
    exit 1
end

echo "ℹ no-to-string: running self-tests..."
if not _self_test
    exit 1
end

echo "ℹ no-to-string: scanning workspace..."
set -l raw (mktemp)
set -l stderr_file (mktemp)
ast-grep scan --config "$config" --json=stream . >"$raw" 2>"$stderr_file"
set -l scan_status $status
if test $scan_status -gt 1
    echo "✗ no-to-string: ast-grep scan failed (exit $scan_status)" >&2
    cat "$stderr_file" >&2
    rm -f "$raw" "$stderr_file"
    exit 1
end
rm -f "$stderr_file"
jq -s -f "$filter" "$raw" | jq -c '.[]' >"$out"
or begin
    echo "✗ no-to-string: jq classification failed" >&2
    rm -f "$raw"
    exit 1
end
rm -f "$raw"

set -l total (string trim (wc -l < "$out"))
if test "$total" -gt 0
    echo "✗ no-to-string: $total owned-string conversion(s):"
    echo
    jq -r '"\(.file):\(.line):\(.column)  no-to-string  \(.message)"' "$out"
    echo
    echo "Per-file counts:"
    jq -r '.file' "$out" | sort | uniq -c | sort -rn
    echo
else
    echo "✓ no-to-string: no owned-string conversions"
end
echo "ℹ no-to-string: JSON stream → $out"

if test "$total" -gt 0
    echo "✗ no-to-string: owned-string conversions found — failing lint" >&2
    exit 1
end
exit 0
