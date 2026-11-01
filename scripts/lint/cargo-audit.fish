#!/usr/bin/env fish
# cargo-audit sub-lint: run `cargo audit` and fail on any vulnerability or
# warning-level advisory (unmaintained, unsound, yanked, notice), matching the
# strictest `[advisories]` policy in deny.toml.
#
# Usage (run from the repo root):
#   fish scripts/lint/cargo-audit.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for one-JSON-object-per-line findings.
#   Defaults to `tmp/cargo-audit.ndjson`. The `just lint` orchestrator passes
#   `tmp/cargo-audit.ndjson`.
#
# Exit status:
#   0 — no vulnerabilities or warning-level advisories.
#   1 — cargo-audit itself failed (missing binary, advisory-DB fetch error,
#       JSON parse failure) or findings were reported.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
cd "$repo_root"

set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/cargo-audit.ndjson"
end
mkdir -p (path dirname "$out")

# Hard dependency: the binary must be present, otherwise the sub-lint would
# falsely report PASS.
if not command -v cargo-audit >/dev/null
    echo "✗ cargo-audit: cargo-audit binary not found in PATH" >&2
    exit 1
end

set -l run_id "cargo-audit-$fish_pid"
set -l json_out "tmp/$run_id.json"
set -l stderr_f "tmp/$run_id.stderr"

echo "ℹ cargo-audit: running..."
# `--deny warnings` turns unmaintained/unsound/yanked/notice advisories into
# hard failures, so cargo-audit's exit code reflects the strict policy.
cargo audit --deny warnings --json >"$json_out" 2>"$stderr_f"
set -l audit_status $status

# Normalize findings (vulnerabilities + every warning bucket) into ndjson.
jq -r '
  ([.vulnerabilities.list[]?]
   + [.warnings.unmaintained[]?]
   + [.warnings.unsound[]?]
   + [.warnings.yanked[]?]
   + [.warnings.notice[]?])
  | .[]
  | {file: "Cargo.lock",
     line: 0,
     ruleId: "cargo-audit",
     message: (.advisory.id + ": " + .package.name + " " + .package.version + " — " + .advisory.title)}
  | @json
' "$json_out" >"$out"
set -l jq_status $status
if test $jq_status -ne 0
    echo "✗ cargo-audit: failed to parse cargo audit JSON" >&2
    cat "$stderr_f" >&2
    exit 1
end

# cargo-audit exit codes: 0 = clean, 1 = findings, 2 = tool error (e.g. the
# advisory DB could not be fetched).
if test $audit_status -gt 1
    echo "✗ cargo-audit: cargo audit failed (exit $audit_status)" >&2
    cat "$stderr_f" >&2
    exit 1
end

set -l count (wc -l <"$out" | string trim)
if test $audit_status -ne 0
    echo "✗ cargo-audit: $count finding(s):"
    jq -r '.message' "$out"
    echo "ℹ cargo-audit: JSON stream → $out"
    exit 1
end

echo "✓ cargo-audit: no vulnerabilities or warning-level advisories"
echo "ℹ cargo-audit: JSON stream → $out"
exit 0
