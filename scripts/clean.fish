#!/usr/bin/env fish
# Remove build artifacts. `just clean` clears target/ and tmp/;
# `just clean --stale` clears only stale ephemeral artifacts.
set -l _script_dir (status dirname)
set -l _self (path resolve "$_script_dir/clean.fish")
set -l _root (path resolve "$_script_dir/..")

source "$_root/scripts/lib/colors.fish"
source "$_root/scripts/lib/nix.fish"

_nix_reexec "$_self" $argv
cd "$_root"

argparse stale -- $argv
or begin
    err "invalid clean arguments"
    exit 2
end

if set -q _flag_stale
    set -l removed 0
    # PID-specific clippy outputs (keep symlinks like tmp/clippy.json).
    for f in tmp/clippy-[0-9]*.*
        test -f "$f"; or continue
        rm -f "$f"
        set removed (math $removed + 1)
    end
    # Stale ephemeral target dirs.
    for d in tmp/targets/*/
        test -d "$d"; or continue
        rm -rf "$d"
        set removed (math $removed + 1)
    end
    ok "Cleaned $removed stale artifact(s)"
else
    rm -rf target/ tmp/
    ok "All build artifacts removed"
end
