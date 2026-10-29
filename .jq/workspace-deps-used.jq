# Emit every dependency name a crate manifest references, one per line
# (input is `taplo get -o json` of that manifest). Used by the caller to build
# the set of referenced names and flag `[workspace.dependencies]` entries no
# manifest inherits. Keep the table list in sync with workspace-deps.jq.
def dep_tables:
  { deps: (.dependencies // {}) },
  { deps: (."dev-dependencies" // {}) },
  { deps: (."build-dependencies" // {}) },
  { deps: (.dev_dependencies // {}) },
  { deps: (.build_dependencies // {}) };

def target_tables:
  (.target // {}) | to_entries[] | .value
  | { deps: (.dependencies // {}) },
    { deps: (."dev-dependencies" // {}) },
    { deps: (."build-dependencies" // {}) },
    { deps: (.dev_dependencies // {}) },
    { deps: (.build_dependencies // {}) };

[ dep_tables, target_tables ] | .[] | .deps | keys_unsorted[]
