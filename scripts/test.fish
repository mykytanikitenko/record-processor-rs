#!/usr/bin/env fish
# Run every workspace test through the shared cargo-rudzio runner, then
# enforce a hard line-coverage gate via llvm-cov. `--threshold N` overrides
# the gate (default 96); `--html` also writes an HTML report to
# tmp/coverage/. A stale cargo-rudzio binary (pre-0.2.0) is caught up front
# via `--build-id`.
set -l _script_dir (status dirname)
set -l _self (path resolve "$_script_dir/test.fish")
set -l _root (path resolve "$_script_dir/..")

source "$_root/scripts/lib/colors.fish"
source "$_root/scripts/lib/nix.fish"
source "$_root/scripts/lib/cargo-env.fish"

_nix_reexec "$_self" $argv
cd "$_root"

# Stale-binary guard. 0.2.0 is the first cargo-rudzio that supports
# `--build-id`; a stale 0.1.2 binary errors here with "unknown subcommand".
cargo rudzio --build-id
or begin
    err "stale cargo-rudzio binary: `cargo rudzio --build-id` failed"
    err "uninstall it (`cargo uninstall cargo-rudzio`) and rebuild via nix"
    exit 1
end

argparse -i html 'threshold=' -- $argv
or begin
    err "invalid test arguments"
    exit 2
end

set -l threshold $_flag_threshold
if test -z "$threshold"
    set threshold 96
end

_cargo_env test

eval (cargo llvm-cov show-env --sh)

cargo rudzio test $argv
or begin
    err "tests failed"
    exit 1
end

set -l report_flags report --fail-under-lines $threshold
if set -q _flag_html
    mkdir -p tmp/coverage
    set report_flags $report_flags --html --output-dir tmp/coverage
end

cargo llvm-cov $report_flags
set -l report_status $status

if test $report_status -ne 0
    set -l actual (cargo llvm-cov report --json | jq -r '[.data[].totals.lines] as $l | (($l | map(.covered) | add) * 100 / ($l | map(.count) | add) * 100 | round) / 100')
    err "line coverage below $threshold% (got $actual%)"
    exit 1
end

info "coverage report generated"
