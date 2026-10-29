#!/usr/bin/env fish
# cargo-deny sub-lint: run `cargo deny check` over all four checks (advisories,
# bans, licenses, sources) and fail on any finding.
#
# Usage (run from the repo root):
#   fish scripts/lint/cargo-deny.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for one-JSON-object-per-line findings.
#   Defaults to `tmp/cargo-deny.ndjson`. The `just lint` orchestrator passes
#   `tmp/cargo-deny.ndjson`.
#
# Exit status:
#   0 — advisories, bans, licenses, and sources all pass.
#   1 — cargo-deny itself failed (missing binary or config) or a check failed.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
cd "$repo_root"

set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/cargo-deny.ndjson"
end
mkdir -p (path dirname "$out")

if not command -v cargo-deny >/dev/null
    echo "✗ cargo-deny: cargo-deny binary not found in PATH" >&2
    exit 1
end
if not test -f "deny.toml"
    echo "✗ cargo-deny: deny.toml not found" >&2
    exit 1
end

set -l run_id "cargo-deny-$fish_pid"
set -l log_f "tmp/$run_id.log"

echo "ℹ cargo-deny: running check (advisories, bans, licenses, sources)..."
cargo deny check >"$log_f" 2>&1
set -l deny_status $status

# Emit one ndjson record per error line. The `error[kind]:` prefix is stable in
# cargo-deny's human output.
grep -E '^error\[[^]]+\]:' "$log_f" \
    | jq -c -R '{file: "deny.toml", line: 0, ruleId: "cargo-deny", message: .}' >"$out"

set -l count (wc -l <"$out" | string trim)
if test $deny_status -ne 0
    echo "✗ cargo-deny: $count error finding(s):"
    jq -r '.message' "$out"
    # Surface the per-check summary line (e.g. "advisories FAILED, bans FAILED").
    grep -E 'advisories (FAILED|ok)' "$log_f" | tail -n 1
    echo "ℹ cargo-deny: full report → $log_f"
    exit 1
end

echo "✓ cargo-deny: advisories, bans, licenses, sources all ok"
echo "ℹ cargo-deny: full report → $log_f"
exit 0
