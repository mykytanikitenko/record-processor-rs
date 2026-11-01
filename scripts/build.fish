#!/usr/bin/env fish
# Build the entire workspace.
set -l _script_dir (status dirname)
set -l _self (path resolve "$_script_dir/build.fish")
set -l _root (path resolve "$_script_dir/..")

source "$_root/scripts/lib/colors.fish"
source "$_root/scripts/lib/nix.fish"
source "$_root/scripts/lib/cargo-env.fish"

_nix_reexec "$_self" $argv
cd "$_root"

set -l scope_flags
if test (count $argv) -gt 0
    set scope_flags $argv
else
    set scope_flags --workspace
end

_cargo_env build
cargo build $scope_flags --all-targets --timings
