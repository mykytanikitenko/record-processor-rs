#!/usr/bin/env fish
# cargo-machete sub-lint: run `cargo machete` and fail on any unused
# dependency.
#
# Usage (run from the repo root):
#   fish scripts/lint/cargo-machete.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for one-JSON-object-per-line findings.
#   Defaults to `tmp/cargo-machete.ndjson`. The `just lint` orchestrator passes
#   `tmp/cargo-machete.ndjson`.
#
# Exit status:
#   0 — no unused dependencies.
#   1 — cargo-machete itself failed (missing binary or parser) or unused
#       dependencies were found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
cd "$repo_root"

set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/cargo-machete.ndjson"
end
mkdir -p (path dirname "$out")

if not command -v cargo-machete >/dev/null
    echo "✗ cargo-machete: cargo-machete binary not found in PATH" >&2
    exit 1
end

# The jq filter is a hard dependency: without it the sub-lint would produce no
# findings and falsely report PASS.
set -l filter_jq "$repo_root/.jq/cargo-machete.jq"
if not test -f "$filter_jq"
    echo "✗ cargo-machete: filter file missing: $filter_jq" >&2
    exit 1
end

set -l run_id "cargo-machete-$fish_pid"
set -l log_f "tmp/$run_id.log"

echo "ℹ cargo-machete: running..."
cargo machete >"$log_f" 2>&1
set -l machete_status $status

# Normalize the human output into ndjson (one record per unused dependency).
jq -c -R -s -f "$filter_jq" "$log_f" >"$out"
set -l jq_status $status
if test $jq_status -ne 0
    echo "✗ cargo-machete: failed to parse cargo machete output" >&2
    cat "$log_f" >&2
    exit 1
end

set -l count (wc -l <"$out" | string trim)
if test $machete_status -ne 0
    echo "✗ cargo-machete: $count unused dependency finding(s):"
    jq -r '.message' "$out"
    echo "ℹ cargo-machete: JSON stream → $out"
    exit 1
end

echo "✓ cargo-machete: no unused dependencies"
echo "ℹ cargo-machete: JSON stream → $out"
exit 0
