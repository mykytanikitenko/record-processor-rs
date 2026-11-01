#!/usr/bin/env fish
# Apply all formatters (Rust, TOML, Markdown, Nix, Justfile, Fish, YAML).
set -l _script_dir (status dirname)
set -l _self (path resolve "$_script_dir/fmt.fish")
set -l _root (path resolve "$_script_dir/..")

source "$_root/scripts/lib/colors.fish"
source "$_root/scripts/lib/nix.fish"
source "$_root/scripts/lib/fish-sources.fish"

_nix_reexec "$_self" $argv
cd "$_root"

info "Formatting Rust..."
cargo fmt --all
or begin
    err "cargo fmt failed"
    exit 1
end

info "Formatting TOML..."
taplo fmt
or begin
    err "taplo fmt failed"
    exit 1
end

info "Formatting Markdown..."
mdformat (git ls-files --cached '*.md')
or begin
    err "mdformat failed"
    exit 1
end

info "Formatting Nix..."
nixfmt flake.nix
or begin
    err "nixfmt failed"
    exit 1
end

info "Formatting Justfile..."
just --fmt --unstable
or begin
    err "just --fmt failed"
    exit 1
end

info "Formatting Fish..."
for f in (_fish_sources)
    fish_indent -w "$f"
    or begin
        err "fish_indent failed for $f"
        exit 1
    end
end

info "Formatting YAML..."
set -l yaml_files (git ls-files --cached '*.yml' '*.yaml')
if test (count $yaml_files) -gt 0
    yamlfmt $yaml_files
    or begin
        err "yamlfmt failed"
        exit 1
    end
end

ok "All formatters applied"
