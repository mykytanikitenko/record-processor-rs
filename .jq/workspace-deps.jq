# Emit one JSON object per non-workspace dependency in a crate `Cargo.toml`
# (input is `taplo get -o json` of that manifest). Output is empty when every
# dependency inherits `workspace = true` with only the permitted extra keys
# (`features`, `optional`).
#
# Cargo reference (verified against these sections before encoding):
#   https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#inheriting-a-dependency-from-a-workspace
#   — `workspace = true` inherits from `[workspace.dependencies]`; alongside it,
#     the only keys Cargo permits are `optional`, `features`, and
#     `default-features` (Edition 2024 / Rust 1.99+); "Other than optional,
#     features, and default-features, inherited dependencies cannot use any
#     other dependency key (such as version)."
# This lint is STRICTER than Cargo: `default-features` / `default_features`
# are also rejected (defaults must be centralized in `[workspace.dependencies]`,
# see https://doc.rust-lang.org/cargo/reference/workspaces.html#the-dependencies-table),
# and `workspace = false` (explicitly not inheriting) is rejected.
#
# The dependency tables covered are `[dependencies]`, `[dev-dependencies]`,
# `[build-dependencies]`, and their legacy underscore spellings
# (`dev_dependencies`, `build_dependencies`), plus every
# `[target.'<cfg>'.<table>]` variant. taplo normalizes the dotted
# (`foo.workspace = true`) and table (`[dependencies.foo]`) TOML forms into the
# same JSON shape, so both are handled by this traversal. Keep the table list
# in sync with workspace-deps-used.jq.
#
# Fields: { section, name, kind, detail }
#   kind ∈ version-string | path | version | git | registry | workspace-false
#        | disallowed-key | non-workspace | unexpected

def classify($name; $v):
  if ($v | type) == "string" then
    { kind: "version-string", detail: $v }
  elif ($v | type) == "object" then
    if $v.workspace == false then
      { kind: "workspace-false", detail: "" }
    elif $v.workspace == true then
      ( ($v | keys_unsorted) | map(select(. != "workspace")) ) as $extra
      | ( $extra | map(select(. != "features" and . != "optional")) ) as $disallowed
      | if ($disallowed | length) > 0
        then { kind: "disallowed-key", detail: ($disallowed | join(", ")) }
        else empty
        end
    elif ($v | has("path")) then
      { kind: "path", detail: ($v.path | tostring) }
    elif ($v | has("git")) then
      { kind: "git", detail: ($v.git | tostring) }
    elif ($v | has("version")) then
      { kind: "version", detail: ($v.version | tostring) }
    elif ($v | has("registry")) then
      { kind: "registry", detail: ($v.registry | tostring) }
    else
      { kind: "non-workspace", detail: "" }
    end
  else
    { kind: "unexpected", detail: ($v | tostring) }
  end;

def dep_tables:
  { section: "dependencies", deps: (.dependencies // {}) },
  { section: "dev-dependencies", deps: (."dev-dependencies" // {}) },
  { section: "build-dependencies", deps: (."build-dependencies" // {}) },
  { section: "dev_dependencies", deps: (.dev_dependencies // {}) },
  { section: "build_dependencies", deps: (.build_dependencies // {}) };

def target_tables:
  (.target // {}) | to_entries[] | .key as $cfg | .value
  | { section: ("target." + $cfg + ".dependencies"), deps: (.dependencies // {}) },
    { section: ("target." + $cfg + ".dev-dependencies"), deps: (."dev-dependencies" // {}) },
    { section: ("target." + $cfg + ".build-dependencies"), deps: (."build-dependencies" // {}) },
    { section: ("target." + $cfg + ".dev_dependencies"), deps: (.dev_dependencies // {}) },
    { section: ("target." + $cfg + ".build_dependencies"), deps: (.build_dependencies // {}) };

[ dep_tables, target_tables ] | .[]
| .section as $section
| .deps | to_entries[]
| classify(.key; .value) as $c
| { section: $section, name: .key, kind: $c.kind, detail: $c.detail }
