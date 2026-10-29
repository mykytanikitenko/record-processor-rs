#!/usr/bin/env fish
# no-suppressions linter: forbid lint suppression across the workspace.
#
# Suppressions are permitted in exactly one place — the root `Cargo.toml`
# `[workspace.lints.*]` table — and forbidden everywhere else. Detection
# channels, all failing (exit 1) on any finding:
#   1. Rust `#[allow(..)]`/`#[expect(..)]` (direct or inside `cfg_attr`) via
#      ast-grep structural matching (scripts/lint/suppressions/rules/…).
#   2. Cargo.toml `[lints.*]` entries whose level is `allow` (string or
#      `{ level = "allow", … }`), parsed with taplo. Root `[workspace.lints.*]`
#      allows are permitted; crate-level `[lints.*]` allows are not. Cargo
#      does not accept an `expect` level in `[lints]` (verified: "unknown
#      variant `expect`"), so only `allow` is checked here.
#   3. clippy.toml `allow-*-in-tests` keys set to `true`.
#
# Usage (run from the repo root):
#   fish scripts/lint/no-suppressions.fish [OUTPUT_NDJSON]
#
# Exit status:
#   0 — self-tests passed and no suppressions were found.
#   1 — self-tests failed, a parser/ast-grep error, or suppressions found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
set -l config (path resolve "$script_dir/suppressions/sgconfig.yml")

cd "$repo_root"

set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/no-suppressions.ndjson"
end
mkdir -p (path dirname "$out")

# Accumulated findings: one compact JSON object string per entry.
set -g findings

function _record --argument-names path line column message
    set -l json (jq -cn --arg f "$path" --argjson l "$line" --argjson c "$column" --arg m "$message" \
        '{file:$f,line:$l,column:$c,ruleId:"no-suppressions",message:$m}')
    set -g findings $findings $json
end

# Locate the 1-based line number of a top-level `key = ...` entry. Returns 0
# when absent. The key is a lint/config name (regex-safe: [a-z0-9_-]+).
function _find_line --argument-names path key
    set -l hit (grep -nE '^[[:space:]]*'"$key"'[[:space:]]*=' "$path" | string replace -r ':.*' '')
    if test (count $hit) -gt 0
        echo $hit[1]
    else
        echo 0
    end
end

# jq program extracting `{scope, tool, lint}` objects whose level is `allow`
# (string shorthand or `{ level = "allow", … }`). `[lints]` and
# `[workspace.lints]` are both inspected; the caller decides whether a given
# scope is permitted based on which manifest is being checked.
set -g toml_jq '[ {scope:"lints", table:(.lints // {})}, {scope:"workspace.lints", table:(.workspace.lints // {})} ] | map( .scope as $scope | [ .table | to_entries[] | select((.value | type) == "object") | .key as $tool | .value | to_entries[] | .key as $lint | .value as $val | select($val == "allow" or (($val | type) == "object" and $val.level == "allow")) | {scope:$scope, tool:$tool, lint:$lint} ] ) | add | .[]'

# Classify one Cargo.toml manifest's lint levels. `$is_root` is 1 for the
# workspace root `Cargo.toml`, whose `[workspace.lints.*]` allows are the one
# permitted location. Shared by the real scan and the runtime self-test.
function _classify_toml --argument-names path is_root
    set -l toml_json (taplo get -f "$path" -o json)
    or begin
        echo "✗ no-suppressions: taplo failed to parse $path" >&2
        return 1
    end
    set -l supp (printf '%s\n' "$toml_json" | jq -c "$toml_jq")
    or begin
        echo "✗ no-suppressions: jq failed on $path" >&2
        return 1
    end
    for entry in $supp
        set -l scope (printf '%s\n' "$entry" | jq -r '.scope')
        set -l tool (printf '%s\n' "$entry" | jq -r '.tool')
        set -l lint (printf '%s\n' "$entry" | jq -r '.lint')
        if test "$scope" = "workspace.lints"; and test "$is_root" -eq 1
            continue
        end
        set -l ln (_find_line "$path" "$lint")
        _record "$path" "$ln" 1 "[$scope.$tool] $lint is set to allow"
    end
end

# Classify one clippy.toml manifest's `allow-*-in-tests = true` keys.
function _classify_clippy_toml --argument-names path
    set -l keys (taplo get -f "$path" -o json | jq -r 'to_entries[] | select(.key | startswith("allow-")) | select(.value == true) | .key')
    or begin
        echo "✗ no-suppressions: failed to parse $path" >&2
        return 1
    end
    for key in $keys
        set -l ln (_find_line "$path" "$key")
        _record "$path" "$ln" 1 "$key = true suppresses clippy lints in tests"
    end
end

# Runtime self-test: generate root/crate manifests + clippy.toml samples, run
# the classifiers against them, and assert the expected flag/not-flag outcome.
function _self_test
    set -g selftest_dir "tmp/nosupp-selftest-$fish_pid"
    set -l dir "$selftest_dir"
    mkdir -p "$dir"

    function _selftest_cleanup --on-event fish_exit
        rm -rf "$selftest_dir"
    end

    # Root [workspace.lints.*] allow → permitted (no finding).
    printf '%s\n' \
        '[workspace]' \
        'members = []' \
        '' \
        '[workspace.lints.clippy]' \
        'too_many_arguments = { level = "allow", priority = 1 }' >"$dir/root-Cargo.toml"
    set -l before (count $findings)
    _classify_toml "$dir/root-Cargo.toml" 1
    or return 1
    if test (count $findings) -ne $before
        echo "✗ no-suppressions: self-test failed — root [workspace.lints] allow flagged" >&2
        return 1
    end

    # Crate [lints.*] allow (object form) → flagged.
    printf '%s\n' \
        '[package]' \
        'name = "crate"' \
        'version = "0.1.0"' \
        '' \
        '[lints.clippy]' \
        'too_many_arguments = { level = "allow", priority = 1 }' >"$dir/crate-Cargo.toml"
    set -l before (count $findings)
    _classify_toml "$dir/crate-Cargo.toml" 0
    or return 1
    if test (count $findings) -eq $before
        echo "✗ no-suppressions: self-test failed — crate [lints] allow not flagged" >&2
        return 1
    end

    # Crate [lints.*] allow (string shorthand) → flagged.
    printf '%s\n' \
        '[package]' \
        'name = "crate2"' \
        'version = "0.1.0"' \
        '' \
        '[lints.rust]' \
        'unsafe_code = "allow"' >"$dir/crate2-Cargo.toml"
    set -l before (count $findings)
    _classify_toml "$dir/crate2-Cargo.toml" 0
    or return 1
    if test (count $findings) -eq $before
        echo "✗ no-suppressions: self-test failed — crate [lints] string allow not flagged" >&2
        return 1
    end

    # clippy.toml allow-*-in-tests = true → flagged.
    printf '%s\n' 'allow-unwrap-in-tests = true' >"$dir/clippy.toml"
    set -l before (count $findings)
    _classify_clippy_toml "$dir/clippy.toml"
    or return 1
    if test (count $findings) -eq $before
        echo "✗ no-suppressions: self-test failed — clippy.toml allow-*=true not flagged" >&2
        return 1
    end

    # clippy.toml allow-*-in-tests = false → not flagged.
    printf '%s\n' 'allow-unwrap-in-tests = false' >"$dir/clippy.toml"
    set -l before (count $findings)
    _classify_clippy_toml "$dir/clippy.toml"
    or return 1
    if test (count $findings) -ne $before
        echo "✗ no-suppressions: self-test failed — clippy.toml allow-*=false flagged" >&2
        return 1
    end

    set -g findings
    echo "✓ no-suppressions: self-tests passed"
    return 0
end

# 1. Self-test the ast-grep rule + runtime classifiers before scanning.
echo "ℹ no-suppressions: running rule self-tests..."
if not ast-grep test --config "$config" --skip-snapshot-tests
    echo "✗ no-suppressions: rule self-tests failed — aborting" >&2
    exit 1
end
if not _self_test
    exit 1
end

# 2. Rust attributes via ast-grep (dedicated config: only the no-suppressions
#    rule, so findings are never double-reported by the panic sub-lint).
echo "ℹ no-suppressions: scanning Rust attributes..."
set -l rust_raw (mktemp)
set -l rust_stderr (mktemp)
ast-grep scan --config "$config" --json=stream . >"$rust_raw" 2>"$rust_stderr"
set -l scan_status $status
if test $scan_status -gt 1
    echo "✗ no-suppressions: ast-grep scan failed (exit $scan_status)" >&2
    cat "$rust_stderr" >&2
    rm -f "$rust_stderr" "$rust_raw"
    exit 1
end
rm -f "$rust_stderr"

if test -s "$rust_raw"
    for line in (jq -c '{file,line:(.range.start.line+1),column:(.range.start.column+1),ruleId,message}' "$rust_raw")
        set -g findings $findings $line
    end
end
rm -f "$rust_raw"

# 3. Cargo.toml lint levels (root `[workspace.lints.*]` permits; `[lints.*]`
#    and non-root `[workspace.lints.*]` allow are violations).
for toml_file in (git ls-files --cached '*Cargo.toml')
    set -l is_root 0
    if test "$toml_file" = "Cargo.toml"
        set is_root 1
    end
    _classify_toml "$toml_file" "$is_root"
    or exit 1
end

# 4. clippy.toml allow-* keys.
if test -f clippy.toml
    _classify_clippy_toml clippy.toml
    or exit 1
end

# 5. Write ndjson output (empty file when clean).
printf '%s\n' $findings >"$out"

# 6. Human-readable diagnostics + summary.
set -l total (count $findings)
if test $total -gt 0
    echo "✗ no-suppressions: $total suppression finding(s):"
    echo
    printf '%s\n' $findings | jq -r '"\(.file):\(.line):\(.column)  no-suppressions  \(.message)"'
    echo
    echo "Per-source counts:"
    printf '%s\n' $findings | jq -r '.file' | sort | uniq -c | sort -rn
    echo
else
    echo "✓ no-suppressions: no lint suppressions found"
end
echo "ℹ no-suppressions: JSON stream → $out"

if test $total -gt 0
    echo "✗ no-suppressions: lint suppressions found — failing lint" >&2
    exit 1
end
exit 0
