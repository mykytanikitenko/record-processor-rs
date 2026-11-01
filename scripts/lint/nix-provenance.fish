#!/usr/bin/env fish
# nix-provenance sub-lint: every flake input is pinned in flake.lock to a full
# commit plus sha256 narHash, every fetchFromGitHub in flake.nix carries a full
# 40-hex rev and a sha256 hash, and the rudzio rev agrees across flake.nix,
# Cargo.toml and deny.toml's git allow-list source.
#
# Usage (run from the repo root):
#   fish scripts/lint/nix-provenance.fish [OUTPUT_NDJSON]
#
# Exit status:
#   0 — self-tests passed and provenance is fully pinned and consistent.
#   1 — self-tests failed, a parse error, or a provenance violation.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
cd "$repo_root"

set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/nix-provenance.ndjson"
end
mkdir -p (path dirname "$out")

set -l filter_jq "$repo_root/.jq/flake-lock.jq"
if not test -f "$filter_jq"
    echo "✗ nix-provenance: filter file missing: $filter_jq" >&2
    exit 1
end

set -g findings

function _record --argument-names path message
    set -l json (jq -cn --arg f "$path" --arg m "$message" \
        '{file:$f,line:0,ruleId:"nix-provenance",message:$m}')
    set -g findings $findings $json
end

# Self-test: a well-pinned lock yields no violations; a lock with a short rev,
# missing narHash and non-github type yields all three.
function _self_test --argument-names filter
    set -l good '{"nodes":{"a":{"locked":{"type":"github","rev":"0123456789abcdef0123456789abcdef01234567","narHash":"sha256-x"}},"root":{"inputs":{"a":"a"}}},"root":"root","version":7}'
    set -l bad '{"nodes":{"a":{"locked":{"type":"git","rev":"abc","narHash":"md5-x"}},"root":{"inputs":{"a":"a"}}},"root":"root","version":7}'
    set -l good_out (printf '%s' "$good" | jq -r -f "$filter")
    or return 1
    if test (count $good_out) -ne 0
        echo "✗ nix-provenance: self-test failed — pinned lock flagged" >&2
        return 1
    end
    set -l bad_out (printf '%s' "$bad" | jq -r -f "$filter")
    or return 1
    if test (count $bad_out) -ne 3
        echo "✗ nix-provenance: self-test failed — unpinned lock not fully flagged" >&2
        return 1
    end
    echo "✓ nix-provenance: self-tests passed"
    return 0
end

echo "ℹ nix-provenance: running self-tests..."
if not _self_test "$filter_jq"
    exit 1
end

# 1. flake.lock: every non-root node pinned.
set -l lock_violations (jq -r -f "$filter_jq" flake.lock)
or begin
    echo "✗ nix-provenance: failed to parse flake.lock" >&2
    exit 1
end
for v in $lock_violations
    _record flake.lock "$v"
end

# 2. flake.nix: every `rev = "..."` is a full 40-hex commit; every
#    fetchFromGitHub has a sha256 hash.
for line in (string match -r '^\s*rev = "[^"]*";' <flake.nix)
    set -l rev (string replace -r '^\s*rev = "([^"]*)";' '$1' -- $line)
    if not string match -qr '^[0-9a-f]{40}$' -- "$rev"
        _record flake.nix "rev \"$rev\" is not a full 40-hex commit"
    end
end
set -l fetches (string match -r 'fetchFromGitHub' <flake.nix | count)
set -l hashes (string match -r '^\s*hash = "sha256-[^"]+";' <flake.nix | count)
if test $fetches -ne $hashes
    _record flake.nix "$fetches fetchFromGitHub call(s) but $hashes sha256 hash(es)"
end

# 3. rudzio rev agreement: flake.nix vs Cargo.toml.
set -l nix_rev (string replace -r '.*rev = "([0-9a-f]{40})".*' '$1' -- (string match -r 'rev = "[0-9a-f]{40}";' <flake.nix | head -n 1))
set -l cargo_rev (string match -rg 'rudzio = \{ git = "[^"]*", rev = "([0-9a-f]{40})"' <Cargo.toml)
if test -z "$cargo_rev"
    _record Cargo.toml "rudzio git dependency with a full 40-hex rev not found"
else if test "$nix_rev" != "$cargo_rev"
    _record flake.nix "rudzio rev $nix_rev differs from Cargo.toml rev $cargo_rev"
end

printf '%s\n' $findings >"$out"
if test (count $findings) -gt 0
    echo "✗ nix-provenance: "(count $findings)" finding(s):"
    printf '%s\n' $findings | jq -r '"\(.file)  nix-provenance  \(.message)"'
    echo "ℹ nix-provenance: JSON stream → $out"
    exit 1
end

echo "✓ nix-provenance: flake inputs pinned and rudzio rev consistent"
echo "ℹ nix-provenance: JSON stream → $out"
exit 0
