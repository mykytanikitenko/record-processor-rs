#!/usr/bin/env fish
# Clippy sub-lint: run cargo clippy, count errors/warnings, fail on any.
# Invoked by scripts/lint.fish (already inside the nix dev shell).
set -l _script_dir (status dirname)
set -l _root (path resolve "$_script_dir/../..")

cd "$_root"

set -l run_id "clippy-$fish_pid"
set -l raw "tmp/$run_id.raw"
set -l ndjson "tmp/$run_id.ndjson"
set -l stderr_f "tmp/$run_id.stderr"
set -l json_out "tmp/$run_id.json"

mkdir -p tmp

# The jq filter files are a hard dependency: if either is absent on disk the
# downstream counts come back empty and the sub-lint would falsely report
# PASS. Fail loudly before running cargo rather than after.
set -l stream_jq "$_root/.jq/clippy-stream.jq"
set -l structured_jq "$_root/.jq/clippy-structured.jq"
if not test -f "$stream_jq"; or not test -f "$structured_jq"
    echo "✗ clippy: filter files missing: $stream_jq / $structured_jq" >&2
    exit 1
end

set -l scope_flags
if test (count $argv) -gt 0
    set scope_flags $argv
else
    set scope_flags --workspace
end

echo "ℹ clippy: running (scope: $scope_flags)..."
cargo clippy $scope_flags --all-targets --all-features --no-deps \
    --message-format=json-diagnostic-rendered-ansi \
    -- -D warnings >"$raw" 2>"$stderr_f"
set -l cargo_status $status

grep '^{' "$raw" >"$ndjson"

# Human-readable diagnostics (rendered cargo diagnostics only).
jq -r -f "$stream_jq" "$ndjson"
or begin
    echo "✗ clippy: jq stream filter failed (exit $status)" >&2
    exit 1
end

# Structured JSON for tooling.
jq -s -f "$structured_jq" "$ndjson" >"$json_out"
or begin
    echo "✗ clippy: jq structured filter failed (exit $status)" >&2
    exit 1
end

ln -sf "$run_id.ndjson" tmp/clippy.ndjson
ln -sf "$run_id.json" tmp/clippy.json
ln -sf "$run_id.raw" tmp/clippy.raw
ln -sf "$run_id.stderr" tmp/clippy.stderr

set -l errors (jq '[.[] | select(.level == "error")] | length' "$json_out")
or begin
    echo "✗ clippy: failed to count errors from structured JSON" >&2
    exit 1
end
set -l warnings (jq '[.[] | select(.level == "warning")] | length' "$json_out")
or begin
    echo "✗ clippy: failed to count warnings from structured JSON" >&2
    exit 1
end

# Counts must be non-negative integers. Anything else (empty string, a jq
# error, malformed JSON) means the numeric comparisons below would lie.
if not string match -qr '^[0-9]+$' -- "$errors"; or not string match -qr '^[0-9]+$' -- "$warnings"
    echo "✗ clippy: counts are not non-negative integers (errors='$errors' warnings='$warnings')" >&2
    exit 1
end

echo "ℹ clippy: $errors errors, $warnings warnings → tmp/clippy.json"

# Cargo itself failed (non-zero exit) without any structured diagnostics:
# a hard compile error, a clippy crash, or a missing tool. Fail loudly.
if test "$cargo_status" -ne 0; and test "$errors" -eq 0; and test "$warnings" -eq 0
    echo "✗ clippy: cargo clippy failed (exit $cargo_status) with no structured diagnostics" >&2
    cat "$stderr_f" >&2
    exit 1
end

# Workspace lints are denied via `-D warnings`, but fail on either level
# regardless so no diagnostic can slip through.
if test "$errors" -gt 0; or test "$warnings" -gt 0
    echo "✗ clippy: $errors errors, $warnings warnings — failing lint" >&2
    exit 1
end

echo "✓ clippy: no errors or warnings"
exit 0
