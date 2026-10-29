# Emit one JSON object per non-workspace dependency in a crate `Cargo.toml`
# (input is `taplo get -o json` of that manifest). Output is empty when every
# dependency inherits `workspace = true` with only the permitted extra keys
# (`features`, `optional`, `default-features`).
#
# Fields: { section, name, kind, detail }
#   kind ∈ version-string | path | version | git | registry | disallowed-key
#        | non-workspace | unexpected

def classify($name; $v):
  if ($v | type) == "string" then
    { kind: "version-string", detail: $v }
  elif ($v | type) == "object" then
    if $v.workspace == true then
      ( $v | keys_unsorted | map(select(. != "workspace")) ) as $extra
      | if ($extra | map(select(. != "features" and . != "optional" and . != "default-features")) | length) > 0
        then { kind: "disallowed-key", detail: ($extra | join(", ")) }
        else empty
        end
    elif ($v | has("path")) then
      { kind: "path", detail: $v.path }
    elif ($v | has("version")) then
      { kind: "version", detail: $v.version }
    elif ($v | has("git")) then
      { kind: "git", detail: $v.git }
    elif ($v | has("registry")) then
      { kind: "registry", detail: $v.registry }
    else
      { kind: "non-workspace", detail: "" }
    end
  else
    { kind: "unexpected", detail: "" }
  end;

def top_tables:
  { section: "dependencies", deps: (.dependencies // {}) },
  { section: "dev-dependencies", deps: (."dev-dependencies" // {}) },
  { section: "build-dependencies", deps: (."build-dependencies" // {}) };

def target_tables:
  (.target // {}) | to_entries[] | .key as $cfg | .value
  | { section: ("target." + $cfg + ".dependencies"), deps: (.dependencies // {}) },
    { section: ("target." + $cfg + ".dev-dependencies"), deps: (."dev-dependencies" // {}) },
    { section: ("target." + $cfg + ".build-dependencies"), deps: (."build-dependencies" // {}) };

[ top_tables, target_tables ] | .[]
| .section as $section
| .deps | to_entries[]
| classify(.key; .value) as $c
| { section: $section, name: .key, kind: $c.kind, detail: $c.detail }
