# Emit findings for the root `[workspace.dependencies]` table (input is
# `taplo get -o json` of the workspace root manifest). Two independent checks
# run per entry, so a single entry can emit more than one finding.
#
# (1) exact-version — every version requirement must be a full exact pin
#     `=MAJOR.MINOR.PATCH` (optional `-pre` / `+build` suffix). A bare `1.2.3`
#     is a caret requirement (`>=1.2.3,<2.0.0`), so it is rejected. Git deps
#     must pin a full 40-hex `rev` (Cargo also accepts short hashes, branches,
#     tags, and named refs — those are rejected here). Path deps carry no
#     version and are fine.
#     https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#version-requirement-syntax
#     https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#choice-of-commit
#
# (2) explicit-features — every entry must be table form with
#     `default-features = false` and an explicit `features = [...]` key (an
#     empty list is allowed), so implicit default features never leak into the
#     build. Applies to registry, git, and path (member) dependencies alike.
#     https://doc.rust-lang.org/cargo/reference/workspaces.html#the-dependencies-table
#
# Fields: { section, name, kind, detail }
#   kind ∈ version-not-exact | git-rev | string-form
#        | missing-default-features-false | missing-features

def exact($v):
  ($v | type) == "string"
  and ($v | test("^=[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.-]+)?(\\+[0-9A-Za-z.-]+)?$"));

def full_sha($v):
  ($v | type) == "string" and ($v | test("^[0-9a-f]{40}$"));

def version_check($v):
  if ($v | type) == "string" then
    if exact($v) then empty else { kind: "version-not-exact", detail: $v } end
  elif ($v | type) == "object" then
    if ($v | has("path")) then
      empty
    elif ($v | has("git")) then
      if ($v | has("rev")) and full_sha($v.rev) then
        empty
      elif ($v | has("rev")) then
        { kind: "git-rev", detail: ("rev \"" + ($v.rev | tostring) + "\" is not a full 40-hex commit SHA") }
      elif ($v | has("branch")) then
        { kind: "git-rev", detail: ("branch \"" + ($v.branch | tostring) + "\" (must pin a full 40-hex rev)") }
      elif ($v | has("tag")) then
        { kind: "git-rev", detail: ("tag \"" + ($v.tag | tostring) + "\" (must pin a full 40-hex rev)") }
      else
        { kind: "git-rev", detail: "no rev/branch/tag (must pin a full 40-hex rev)" }
      end
    elif ($v | has("version")) then
      if exact($v.version) then empty else { kind: "version-not-exact", detail: ($v.version | tostring) } end
    else
      empty
    end
  else
    { kind: "version-not-exact", detail: ($v | tostring) }
  end;

def features_check($v):
  if ($v | type) == "string" then
    { kind: "string-form", detail: $v }
  elif ($v | type) == "object" then
    ( if ($v | has("default-features")) and ($v."default-features" == false)
      then empty
      else { kind: "missing-default-features-false", detail: "" }
      end ),
    ( if ($v | has("features"))
      then empty
      else { kind: "missing-features", detail: "" }
      end )
  else
    { kind: "string-form", detail: ($v | tostring) }
  end;

.workspace.dependencies // {}
| to_entries[]
| .key as $name
| .value as $v
| ( version_check($v), features_check($v) )
| { section: "workspace.dependencies", name: $name, kind: .kind, detail: .detail }
