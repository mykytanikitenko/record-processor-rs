#!/usr/bin/env fish
# workspace-deps linter: every dependency declared in a crate manifest must
# inherit from the root `[workspace.dependencies]` via `{ workspace = true }`
# (dotted `name.workspace = true` is also accepted). Permitted extra keys
# alongside `workspace = true` are `features`, `optional`, and
# `default-features` (verified against Cargo 1.99.1). Any inline `version`,
# `version =` string, `path`, `git`, or `registry` is a violation: those
# belong in the root workspace manifest, not a crate manifest.
#
# Usage (run from the repo root):
#   fish scripts/lint/workspace-deps.fish [OUTPUT_NDJSON]
#
# Exit status:
#   0 — self-tests passed and every crate dependency inherits from workspace.
#   1 — self-tests failed, a parser/jq error, or a non-workspace dep found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")

cd "$repo_root"

set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/workspace-deps.ndjson"
end
mkdir -p (path dirname "$out")

# The jq classifier is a hard dependency: if absent, the sub-lint would
# produce no findings and falsely report PASS. Fail loudly instead.
set -g filter_jq "$repo_root/.jq/workspace-deps.jq"
if not test -f "$filter_jq"
    echo "✗ workspace-deps: filter file missing: $filter_jq" >&2
    exit 1
end

# Accumulated findings: one compact JSON object string per entry.
set -g findings

function _record --argument-names path line column message
    set -l json (jq -cn --arg f "$path" --argjson l "$line" --argjson c "$column" --arg m "$message" \
        '{file:$f,line:$l,column:$c,ruleId:"workspace-deps",message:$m}')
    set -g findings $findings $json
end

# Locate the 1-based line number of a dependency key in a manifest. Matches
# inline (`name = …`), dotted (`name.workspace = …`), and table (`[….name]`)
# forms. Returns 0 when absent.
function _find_dep_line --argument-names path name
    set -l hit (grep -nE '(^[[:space:]]*'"$name"'[[:space:]]*=)|(^[[:space:]]*'"$name"'\.)|(^[[:space:]]*\[[^]]*\.'"$name"'\])' "$path" | string replace -r ':.*' '')
    if test (count $hit) -gt 0
        echo $hit[1]
    else
        echo 0
    end
end

# Human-readable reason for a violation kind.
function _describe --argument-names kind detail
    switch "$kind"
        case version-string
            echo "inline version string \"$detail\" (must be workspace = true)"
        case path
            echo "path dependency \"$detail\" (must be workspace = true)"
        case version
            echo "inline version \"$detail\" (must be workspace = true)"
        case git
            echo "git dependency \"$detail\" (must be workspace = true)"
        case registry
            echo "registry dependency \"$detail\" (must be workspace = true)"
        case disallowed-key
            echo "disallowed key(s) alongside workspace = true: $detail"
        case non-workspace
            echo "not a workspace dependency (missing workspace = true)"
        case '*'
            echo "unexpected dependency form"
    end
end

# Classify one crate manifest. Shared by the real scan and the runtime
# self-test so both exercise identical detection logic.
function _classify_manifest --argument-names path
    set -l dep_json (taplo get -f "$path" -o json)
    or begin
        echo "✗ workspace-deps: taplo failed to parse $path" >&2
        return 1
    end
    set -l violations (printf '%s\n' "$dep_json" | jq -c -f "$filter_jq")
    or begin
        echo "✗ workspace-deps: jq failed on $path" >&2
        return 1
    end
    for v in $violations
        set -l section (printf '%s\n' "$v" | jq -r '.section')
        set -l name (printf '%s\n' "$v" | jq -r '.name')
        set -l kind (printf '%s\n' "$v" | jq -r '.kind')
        set -l detail (printf '%s\n' "$v" | jq -r '.detail')
        set -l ln (_find_dep_line "$path" "$name")
        _record "$path" "$ln" 1 "[$section] $name: "(_describe "$kind" "$detail")
    end
end

# Runtime self-test: generate crate manifests with should-flag / should-pass
# dependency forms and assert the classifier's outcome on each.
function _self_test
    set -g selftest_dir "tmp/wsdeps-selftest-$fish_pid"
    set -l dir "$selftest_dir"
    mkdir -p "$dir"

    function _selftest_cleanup --on-event fish_exit
        rm -rf "$selftest_dir"
    end

    # should-pass: workspace = true with permitted extra keys.
    printf '%s\n' \
        '[package]' \
        'name = "pass"' \
        'version = "0.1.0"' \
        '' \
        '[dependencies]' \
        'foo = { workspace = true }' \
        'bar = { workspace = true, features = ["x"], optional = true, default-features = false }' \
        'baz.workspace = true' \
        '' \
        '[dev-dependencies]' \
        'qux = { workspace = true }' >"$dir/pass-Cargo.toml"
    set -l before (count $findings)
    _classify_manifest "$dir/pass-Cargo.toml"
    or return 1
    if test (count $findings) -ne $before
        echo "✗ workspace-deps: self-test failed — workspace = true dep flagged" >&2
        return 1
    end

    # should-flag: version string, inline version, path, git, registry,
    # disallowed key alongside workspace = true.
    printf '%s\n' \
        '[package]' \
        'name = "flag"' \
        'version = "0.1.0"' \
        '' \
        '[dependencies]' \
        'a = "=1.2.3"' \
        'b = { version = "=1.2.3" }' \
        'c = { path = "../c" }' \
        'd = { git = "https://example.com/d" }' \
        'e = { registry = "my-reg" }' \
        'f = { workspace = true, version = "=1.2.3" }' >"$dir/flag-Cargo.toml"
    set -l before (count $findings)
    _classify_manifest "$dir/flag-Cargo.toml"
    or return 1
    if test (count $findings) -lt (math $before + 6)
        echo "✗ workspace-deps: self-test failed — non-workspace dep not flagged" >&2
        return 1
    end

    set -g findings
    echo "✓ workspace-deps: self-tests passed"
    return 0
end

# 1. Self-test before scanning — a broken detector must fail loudly.
echo "ℹ workspace-deps: running self-tests..."
if not _self_test
    exit 1
end

# 2. Scan every crate manifest.
echo "ℹ workspace-deps: scanning crate manifests..."
for manifest in (git ls-files --cached Cargo.toml 'crates/*/Cargo.toml')
    _classify_manifest "$manifest"
    or exit 1
end

# 3. Write ndjson output (empty file when clean).
printf '%s\n' $findings >"$out"

# 4. Human-readable diagnostics + summary.
set -l total (count $findings)
if test $total -gt 0
    echo "✗ workspace-deps: $total non-workspace dependency finding(s):"
    echo
    printf '%s\n' $findings | jq -r '"\(.file):\(.line):\(.column)  workspace-deps  \(.message)"'
    echo
else
    echo "✓ workspace-deps: all crate dependencies inherit from workspace"
end
echo "ℹ workspace-deps: JSON stream → $out"

if test $total -gt 0
    echo "✗ workspace-deps: non-workspace dependencies found — failing lint" >&2
    exit 1
end
exit 0
