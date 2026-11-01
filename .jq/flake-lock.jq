# Emit one violation message per line for a flake.lock document: every
# non-root node must be a github input locked to a full 40-hex commit with a
# sha256 narHash.
.nodes
| to_entries[]
| select(.key != "root")
| .key as $name
| .value as $node
| (
    if (($node.locked.type // "") != "github")
    then "\($name): locked.type must be github"
    else empty
    end
  ),
  (
    if ((($node.locked.rev // "") | test("^[0-9a-f]{40}$")) | not)
    then "\($name): locked.rev is not a full 40-hex commit"
    else empty
    end
  ),
  (
    if ((($node.locked.narHash // "") | startswith("sha256-")) | not)
    then "\($name): locked.narHash missing or not sha256"
    else empty
    end
  )
