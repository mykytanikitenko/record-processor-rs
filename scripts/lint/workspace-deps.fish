#!/usr/bin/env fish
# workspace-deps linter: every dependency declared in a crate manifest must
# inherit from the root `[workspace.dependencies]` via `{ workspace = true }`
# (dotted `name.workspace = true` is also accepted). Permitted extra keys
# alongside `workspace = true` are `features` and `optional` only.
#
# Verified against the Cargo Book before encoding:
#   • https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#inheriting-a-dependency-from-a-workspace
#     — `workspace = true` inherits from `[workspace.dependencies]`; the only
#       keys Cargo permits alongside it are `optional`, `features`, and
#       `default-features` (Edition 2024 / Rust 1.99+): "Other than optional,
#       features, and default-features, inherited dependencies cannot use any
#       other dependency key (such as version)."
#   • https://doc.rust-lang.org/cargo/reference/workspaces.html#the-dependencies-table
#     — `[workspace.dependencies]` is the definition site; `default-features`
#       declared on a package dependency overrides the workspace value in
#       Edition 2024 packages.
#   • https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#version-requirement-syntax
#     — a bare `1.2.3` is a caret requirement (`>=1.2.3,<2.0.0`); only `=1.2.3`
#       is an exact pin.
#   • https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#choice-of-commit
#     — a git dependency picks its commit via `rev`, `tag`, or `branch`; Cargo
#       accepts short hashes and named refs, so the full-40-hex `rev` rule here
#       is deliberately stricter.
#
# This lint is stricter than Cargo in three ways, all project rules:
#   • `default-features` / `default_features` are FORBIDDEN alongside
#     `workspace = true` (defaults belong in `[workspace.dependencies]`).
#   • `workspace = false` (explicitly not inheriting) is FORBIDDEN.
#   • every `[workspace.dependencies]` entry must be table form with
#     `default-features = false` and an explicit `features = [...]` list, and
#     its version (or git `rev`) must be a full exact pin.
#
# The scan covers `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`,
# their legacy underscore spellings (`dev_dependencies`, `build_dependencies`),
# and every `[target.'<cfg>'.<table>]` variant. taplo normalizes the dotted
# (`foo.workspace = true`) and table (`[dependencies.foo]`) TOML forms into the
# same JSON shape, so both are handled by the same traversal.
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

# The jq classifiers are hard dependencies: if any is absent, the sub-lint
# would produce no findings and falsely report PASS. Fail loudly instead.
set -g filter_jq "$repo_root/.jq/workspace-deps.jq"
set -g root_jq "$repo_root/.jq/workspace-deps-root.jq"
set -g used_jq "$repo_root/.jq/workspace-deps-used.jq"
for f in $filter_jq $root_jq $used_jq
    if not test -f "$f"
        echo "✗ workspace-deps: filter file missing: $f" >&2
        exit 1
    end
end

# Accumulated findings: one compact JSON object string per entry, plus the
# names referenced across manifests and the root workspace keys.
set -g findings
set -g referenced_names
set -g workspace_keys
set -g root_path ""

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
        case workspace-false
            echo "workspace = false (must be workspace = true)"
        case disallowed-key
            echo "disallowed key(s) alongside workspace = true: $detail"
        case non-workspace
            echo "not a workspace dependency (missing workspace = true)"
        case version-not-exact
            echo "non-exact version \"$detail\" (must be an exact =MAJOR.MINOR.PATCH pin)"
        case git-rev
            echo "$detail"
        case string-form
            echo "string form \"$detail\" (must be a table with default-features = false and features = [...])"
        case missing-default-features-false
            echo "missing default-features = false (declare it so default features never leak into the build)"
        case missing-features
            echo "missing features = [...] (list every feature used)"
        case '*'
            echo "unexpected dependency form"
    end
end

function _reset_state
    set -g findings
    set -g referenced_names
    set -g workspace_keys
    set -g root_path ""
end

# Classify one crate manifest's package-level dependency tables and collect the
# dependency names it references into `referenced_names`.
function _classify_manifest --argument-names path
    set -l dep_json (taplo get -f "$path" -o json)
    or begin
        echo "✗ workspace-deps: taplo failed to parse $path" >&2
        return 1
    end
    set -l violations (printf '%s\n' "$dep_json" | jq -c -f "$filter_jq")
    or begin
        echo "✗ workspace-deps: jq (classify) failed on $path" >&2
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
    set -l names (printf '%s\n' "$dep_json" | jq -r -f "$used_jq")
    or begin
        echo "✗ workspace-deps: jq (used) failed on $path" >&2
        return 1
    end
    for n in $names
        set -g -a referenced_names "$n"
    end
    return 0
end

# Classify the root `[workspace.dependencies]` table: exact-version and
# explicit-features checks.
function _classify_root --argument-names path
    set -l dep_json (taplo get -f "$path" -o json)
    or begin
        echo "✗ workspace-deps: taplo failed to parse $path" >&2
        return 1
    end
    set -l violations (printf '%s\n' "$dep_json" | jq -c -f "$root_jq")
    or begin
        echo "✗ workspace-deps: jq (root) failed on $path" >&2
        return 1
    end
    for v in $violations
        set -l name (printf '%s\n' "$v" | jq -r '.name')
        set -l kind (printf '%s\n' "$v" | jq -r '.kind')
        set -l detail (printf '%s\n' "$v" | jq -r '.detail')
        set -l ln (_find_dep_line "$path" "$name")
        _record "$path" "$ln" 1 "[workspace.dependencies] $name: "(_describe "$kind" "$detail")
    end
    return 0
end

# Flag every root `[workspace.dependencies]` key no manifest inherits.
function _report_unused
    set -l refs (printf '%s\n' $referenced_names | sort -u)
    for key in $workspace_keys
        if not contains -- $key $refs
            set -l ln (_find_dep_line "$root_path" "$key")
            _record "$root_path" "$ln" 1 "[workspace.dependencies] $key: unused — no manifest inherits it (workspace = true)"
        end
    end
end

# Scan a set of manifests end-to-end: classify every manifest, detect the root
# (the manifest carrying `[workspace.dependencies]`), run the root checks, then
# report unused workspace entries.
function _scan_manifests
    _reset_state
    for manifest in $argv
        _classify_manifest "$manifest"; or return 1
        set -l dj (taplo get -f "$manifest" -o json); or return 1
        if printf '%s\n' "$dj" | jq -e '.workspace.dependencies // null | type == "object"' >/dev/null 2>&1
            set -g root_path "$manifest"
            set -g workspace_keys (printf '%s\n' "$dj" | jq -r '.workspace.dependencies | keys_unsorted[]')
            _classify_root "$manifest"; or return 1
        end
    end
    if test -n "$root_path"
        _report_unused
    end
    return 0
end

# Runtime self-test: generate crate manifests with should-flag / should-pass
# forms for every check and assert the detector's outcome on each.
function _self_test
    set -g selftest_dir "tmp/wsdeps-selftest-$fish_pid"
    set -l dir "$selftest_dir"
    mkdir -p "$dir"

    function _selftest_cleanup --on-event fish_exit
        rm -rf "$selftest_dir"
    end

    # 1. Classifier should-pass: workspace = true with permitted extra keys,
    #    dotted and table forms, and every dependency table (hyphen and
    #    underscore, top-level and target-scoped).
    set -l pass_toml '[package]
name = "pass"
version = "0.1.0"

[dependencies]
foo = { workspace = true }
bar = { workspace = true, features = ["x"], optional = true }
baz.workspace = true

[dependencies.qux]
workspace = true

[dev-dependencies]
a = { workspace = true }

[build-dependencies]
b = { workspace = true }

[dev_dependencies]
c = { workspace = true }

[build_dependencies]
d = { workspace = true }

[target."cfg(unix)".dependencies]
e = { workspace = true }

[target."cfg(unix)".dev-dependencies]
f = { workspace = true }

[target."cfg(unix)".build-dependencies]
g = { workspace = true }

[target."cfg(unix)".dev_dependencies]
h = { workspace = true }

[target."cfg(unix)".build_dependencies]
i = { workspace = true }
'
    printf '%s' "$pass_toml" >"$dir/pass-Cargo.toml"
    _reset_state
    _classify_manifest "$dir/pass-Cargo.toml"; or return 1
    if test (count $findings) -ne 0
        echo "✗ workspace-deps: self-test failed — workspace = true dep flagged" >&2
        return 1
    end

    # 2. Classifier should-flag: every non-workspace and disallowed-key form.
    set -l flag_toml '[package]
name = "flag"
version = "0.1.0"

[dependencies]
a = "=1.2.3"
b = { version = "=1.2.3" }
c = { path = "../c" }
d = { git = "https://example.com/d" }
e = { registry = "my-reg" }
f = { workspace = true, version = "=1.2.3" }
g = { workspace = true, default-features = false }
h = { workspace = true, default_features = false }
i = { workspace = true, package = "real" }
j = { workspace = false }

[dev_dependencies]
k = "1.0"

[build_dependencies]
l = { version = "1.0" }
'
    printf '%s' "$flag_toml" >"$dir/flag-Cargo.toml"
    _reset_state
    _classify_manifest "$dir/flag-Cargo.toml"; or return 1
    if test (count $findings) -lt 12
        echo "✗ workspace-deps: self-test failed — non-workspace dep not flagged" >&2
        return 1
    end

    # 3. Root should-pass: exact pins, path dep, git + full 40-hex rev, and
    #    explicit default-features = false + features = [...] everywhere.
    set -l root_pass_toml '[workspace]
members = ["crates/*"]

[workspace.dependencies]
a = { version = "=1.2.3", default-features = false, features = [] }
b = { version = "=0.19.2", default-features = false, features = ["x"] }
c = { version = "=1.0.0-rc.1", default-features = false, features = [] }
d = { path = "crates/d", default-features = false, features = [] }
e = { git = "https://example.com/e", rev = "0123456789abcdef0123456789abcdef01234567", default-features = false, features = [] }
'
    printf '%s' "$root_pass_toml" >"$dir/root-pass-Cargo.toml"
    _reset_state
    _classify_root "$dir/root-pass-Cargo.toml"; or return 1
    if test (count $findings) -ne 0
        echo "✗ workspace-deps: self-test failed — exact/features dep flagged" >&2
        return 1
    end

    # 4. Root should-flag: non-exact versions, git without a full 40-hex rev,
    #    string form, and missing/true default-features or missing features.
    set -l root_flag_toml '[workspace]
members = ["crates/*"]

[workspace.dependencies]
a = "1.2.3"
b = { version = "^1.2.3" }
c = { version = "~1.2" }
d = { version = ">= 1.2.0" }
e = { version = "*" }
f = { version = "1.*" }
g = { version = "1" }
h = { version = ">= 1.2, < 2" }
i = { git = "https://example.com/i", branch = "main", default-features = false, features = [] }
j = { git = "https://example.com/j", rev = "4c59b707", default-features = false, features = [] }
k = { git = "https://example.com/k", default-features = false, features = [] }
l = { version = "=1.2.3", default-features = false }
m = { version = "=1.2.3", features = [] }
n = { version = "=1.2.3", default-features = true, features = [] }
'
    printf '%s' "$root_flag_toml" >"$dir/root-flag-Cargo.toml"
    _reset_state
    _classify_root "$dir/root-flag-Cargo.toml"; or return 1
    if test (count $findings) -lt 20
        echo "✗ workspace-deps: self-test failed — non-exact/non-explicit dep not flagged" >&2
        return 1
    end

    # 5. Unused workspace dep: should-pass (all keys referenced).
    set -l used_root_pass '[workspace]
members = ["member"]

[workspace.dependencies]
used = { version = "=1.2.3", default-features = false, features = [] }
'
    set -l used_member '[package]
name = "member"
version = "0.1.0"

[dependencies]
used = { workspace = true }
'
    printf '%s' "$used_root_pass" >"$dir/used-root-pass-Cargo.toml"
    printf '%s' "$used_member" >"$dir/used-member-Cargo.toml"
    _reset_state
    _scan_manifests "$dir/used-root-pass-Cargo.toml" "$dir/used-member-Cargo.toml"; or return 1
    if test (count $findings) -ne 0
        echo "✗ workspace-deps: self-test failed — used workspace dep flagged as unused" >&2
        return 1
    end

    # 6. Unused workspace dep: should-flag (one key no manifest inherits).
    set -l used_root_flag '[workspace]
members = ["member"]

[workspace.dependencies]
used = { version = "=1.2.3", default-features = false, features = [] }
unused = { version = "=1.2.3", default-features = false, features = [] }
'
    printf '%s' "$used_root_flag" >"$dir/used-root-flag-Cargo.toml"
    _reset_state
    _scan_manifests "$dir/used-root-flag-Cargo.toml" "$dir/used-member-Cargo.toml"; or return 1
    if test (count $findings) -lt 1
        echo "✗ workspace-deps: self-test failed — unused workspace dep not flagged" >&2
        return 1
    end

    _reset_state
    echo "✓ workspace-deps: self-tests passed"
    return 0
end

# 1. Self-test before scanning — a broken detector must fail loudly.
echo "ℹ workspace-deps: running self-tests..."
if not _self_test
    exit 1
end

# 2. Scan every tracked Cargo.toml, root `[package]` tables included;
#    `[workspace.dependencies]` is the definition site and is checked by the
#    root rules, not the member classifier.
echo "ℹ workspace-deps: scanning crate manifests..."
set -l manifests (git ls-files --cached '*Cargo.toml')
if test (count $manifests) -eq 0
    echo "✗ workspace-deps: no tracked Cargo.toml files found" >&2
    exit 1
end
_scan_manifests $manifests
or exit 1

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
