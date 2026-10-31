#!/usr/bin/env fish
# Full lint pipeline: format check + fish syntax + clippy + panic + non-fish +
# no-suppressions + no-banner-comments + workspace-deps + yaml-lint + recipe
# lint + non-rudzio + no-fallbacks + module-only + no-new-get +
# no-static-lifetime + no-tokio-runtime + no-manual-fmt + no-std-io +
# no-std-fs + no-implicit-alloc + no-materialize + cargo-audit + cargo-deny +
# cargo-machete + nix-provenance + forbidden-word. Rust findings are annotated
# in-place with
# TODO(lint/<rule>) markers, then a TODO-precedence pass reports acknowledged
# violations through the todo-comments rule. Fails while any finding remains.
set -l _script_dir (status dirname)
set -l _self (path resolve "$_script_dir/lint.fish")
set -l _root (path resolve "$_script_dir/..")

source "$_root/scripts/lib/colors.fish"
source "$_root/scripts/lib/nix.fish"
source "$_root/scripts/lib/fish-sources.fish"

_nix_reexec "$_self" $argv
cd "$_root"

# (a) Format check (all formatters, read-only, no error swallowing)
set -l fmt_failed 0
cargo fmt --all -- --check
or begin
    err "Rust formatting check failed"
    set fmt_failed 1
end
taplo check
or begin
    err "TOML formatting check failed"
    set fmt_failed 1
end
mdformat --check (git ls-files --cached '*.md')
or begin
    err "Markdown formatting check failed"
    set fmt_failed 1
end
nixfmt --check flake.nix
or begin
    err "Nix formatting check failed"
    set fmt_failed 1
end
just --fmt --check --unstable
or begin
    err "Justfile formatting check failed"
    set fmt_failed 1
end
for f in (_fish_sources)
    fish_indent --check "$f"
    or begin
        err "fish_indent check failed for $f"
        set fmt_failed 1
    end
end
set -l yaml_files (git ls-files --cached '*.yml' '*.yaml')
if test (count $yaml_files) -gt 0
    yamlfmt -lint $yaml_files
    or begin
        err "YAML formatting check failed"
        set fmt_failed 1
    end
end
if test $fmt_failed -eq 0
    ok "format check passed"
end

# (b) Fish syntax
set -l fishsyntax_status 0
for f in (_fish_sources)
    fish --no-execute "$f"
    or begin
        err "fish syntax check failed for $f"
        set fishsyntax_status 1
    end
end
if test $fishsyntax_status -eq 0
    ok "fish syntax check passed"
end

# (c) Clippy
set -l clippy_status 1
fish "$_root/scripts/lint/clippy.fish" $argv
set clippy_status $status

# (d) Panic-pattern lint
set -l panic_status 1
fish "$_root/scripts/lint/panic.fish" "$_root/tmp/panic.ndjson"
set panic_status $status

# (e) Non-fish script lint
set -l nonfish_status 1
fish "$_root/scripts/lint/non-fish.fish" "$_root/tmp/non-fish.ndjson"
set nonfish_status $status

# (f) YAML lint
set -l yaml_status 1
if test (count $yaml_files) -gt 0
    yamllint -s -c .yamllint $yaml_files
    set yaml_status $status
else
    set yaml_status 0
end
if test $yaml_status -eq 0
    ok "yaml lint passed"
end

# (g) Justfile recipe lint
set -l recipe_status 1
set -l actual (just --dump --dump-format json --unstable | jq -r '.recipes | keys[]' | sort)
set -l expected build clean fmt lint lint-no-implicit-alloc lint-no-manual-fmt lint-no-materialize lint-no-std-fs lint-no-std-io lint-no-tokio-runtime test
if test (string join ' ' $actual) = (string join ' ' $expected)
    ok "recipe lint passed"
    set recipe_status 0
else
    err "recipe lint failed"
    for n in $expected
        contains -- $n $actual; or err "  missing recipe: $n"
    end
    for n in $actual
        contains -- $n $expected; or err "  unexpected recipe: $n"
    end
end

# (h) no-suppressions lint
set -l nosupp_status 1
fish "$_root/scripts/lint/no-suppressions.fish" "$_root/tmp/no-suppressions.ndjson"
set nosupp_status $status

# (i) no-banner-comments lint
set -l nobanner_status 1
fish "$_root/scripts/lint/no-banner-comments.fish" "$_root/tmp/no-banner-comments.ndjson"
set nobanner_status $status

# (j) workspace-deps lint
set -l wsdeps_status 1
fish "$_root/scripts/lint/workspace-deps.fish" "$_root/tmp/workspace-deps.ndjson"
set wsdeps_status $status

# (k) non-rudzio test lint
set -l nonrudzio_status 1
fish "$_root/scripts/lint/non-rudzio.fish" "$_root/tmp/non-rudzio.ndjson"
set nonrudzio_status $status

# (l) no-fallbacks lint
set -l nofallbacks_status 1
fish "$_root/scripts/lint/no-fallbacks.fish" "$_root/tmp/no-fallbacks.ndjson"
set nofallbacks_status $status

# (l.1) shared-error-types lint
set -l seterr_status 1
fish "$_root/scripts/lint/shared-error-types.fish" "$_root/tmp/shared-error-types.ndjson"
set seterr_status $status

# (m) module-only lint
set -l moduleonly_status 1
fish "$_root/scripts/lint/module-only.fish" "$_root/tmp/module-only.ndjson"
set moduleonly_status $status

# (n) no-new-get lint
set -l nonewget_status 1
fish "$_root/scripts/lint/no-new-get.fish" "$_root/tmp/no-new-get.ndjson"
set nonewget_status $status

# (o) no-static-lifetime lint
set -l nostaticlifetime_status 1
fish "$_root/scripts/lint/no-static-lifetime.fish" "$_root/tmp/no-static-lifetime.ndjson"
set nostaticlifetime_status $status

# (o.0) no-tokio-runtime lint
set -l notokio_status 1
fish "$_root/scripts/lint/no-tokio-runtime.fish" "$_root/tmp/no-tokio-runtime.ndjson"
set notokio_status $status

# (o.0.1) no-manual-fmt lint
set -l nomanualfmt_status 1
fish "$_root/scripts/lint/no-manual-fmt.fish" "$_root/tmp/no-manual-fmt.ndjson"
set nomanualfmt_status $status

# (o.0.2) no-std-io lint
set -l nostdio_status 1
fish "$_root/scripts/lint/no-std-io.fish" "$_root/tmp/no-std-io.ndjson"
set nostdio_status $status

# (o.0.2.1) no-std-fs lint
set -l nostdfs_status 1
fish "$_root/scripts/lint/no-std-fs.fish" "$_root/tmp/no-std-fs.ndjson"
set nostdfs_status $status

# (o.0.3) no-implicit-alloc lint
set -l noimplicitalloc_status 1
fish "$_root/scripts/lint/no-implicit-alloc.fish" "$_root/tmp/no-implicit-alloc.ndjson"
set noimplicitalloc_status $status

# (o.0.4) no-materialize lint
set -l nomaterialize_status 1
fish "$_root/scripts/lint/no-materialize.fish" "$_root/tmp/no-materialize.ndjson"
set nomaterialize_status $status

# (o.1) cargo-audit (RustSec advisories, strictest deny policy)
set -l audit_status 1
fish "$_root/scripts/lint/cargo-audit.fish" "$_root/tmp/cargo-audit.ndjson"
set audit_status $status

# (o.2) cargo-deny (licenses / bans / sources / advisories)
set -l deny_status 1
fish "$_root/scripts/lint/cargo-deny.fish" "$_root/tmp/cargo-deny.ndjson"
set deny_status $status

# (o.3) cargo-machete (unused dependencies)
set -l machete_status 1
fish "$_root/scripts/lint/cargo-machete.fish" "$_root/tmp/cargo-machete.ndjson"
set machete_status $status

# (o.4) nix-provenance (flake inputs pinned, rudzio rev consistent)
set -l nixprov_status 1
fish "$_root/scripts/lint/nix-provenance.fish" "$_root/tmp/nix-provenance.ndjson"
set nixprov_status $status

# (o.5) forbidden-word lint (non-Rust: content + commit-history scan). Findings
#       are reported directly here; unlike the Rust sub-lints they are not
#       annotated in-place (the scan covers non-Rust files too).
set -l forbiddenword_status 1
fish "$_root/scripts/lint/forbidden-word.fish" "$_root/tmp/forbidden-word.ndjson"
set forbiddenword_status $status

# (p) Annotate Rust findings in-place (idempotent; mutates crates/ and src/). Runs by
#     default on every `just lint` so violations are impossible to miss.
fish "$_root/scripts/lint/annotate.fish"

# (q) Re-run the cheap Rust scans so the freshly inserted TODO markers are
#     visible to the todo-comments rule and the precedence view is current.
fish "$_root/scripts/lint/panic.fish" "$_root/tmp/panic.ndjson" >/dev/null 2>&1
set panic_status $status
fish "$_root/scripts/lint/non-rudzio.fish" "$_root/tmp/non-rudzio.ndjson" >/dev/null 2>&1
set nonrudzio_status $status
fish "$_root/scripts/lint/no-suppressions.fish" "$_root/tmp/no-suppressions.ndjson" >/dev/null 2>&1
set nosupp_status $status
fish "$_root/scripts/lint/no-banner-comments.fish" "$_root/tmp/no-banner-comments.ndjson" >/dev/null 2>&1
set nobanner_status $status
fish "$_root/scripts/lint/no-fallbacks.fish" "$_root/tmp/no-fallbacks.ndjson" >/dev/null 2>&1
set nofallbacks_status $status
fish "$_root/scripts/lint/shared-error-types.fish" "$_root/tmp/shared-error-types.ndjson" >/dev/null 2>&1
set seterr_status $status
fish "$_root/scripts/lint/module-only.fish" "$_root/tmp/module-only.ndjson" >/dev/null 2>&1
set moduleonly_status $status
fish "$_root/scripts/lint/no-new-get.fish" "$_root/tmp/no-new-get.ndjson" >/dev/null 2>&1
set nonewget_status $status
fish "$_root/scripts/lint/no-static-lifetime.fish" "$_root/tmp/no-static-lifetime.ndjson" >/dev/null 2>&1
set nostaticlifetime_status $status
fish "$_root/scripts/lint/no-tokio-runtime.fish" "$_root/tmp/no-tokio-runtime.ndjson" >/dev/null 2>&1
set notokio_status $status
fish "$_root/scripts/lint/no-manual-fmt.fish" "$_root/tmp/no-manual-fmt.ndjson" >/dev/null 2>&1
set nomanualfmt_status $status
fish "$_root/scripts/lint/no-std-io.fish" "$_root/tmp/no-std-io.ndjson" >/dev/null 2>&1
set nostdio_status $status
fish "$_root/scripts/lint/no-std-fs.fish" "$_root/tmp/no-std-fs.ndjson" >/dev/null 2>&1
set nostdfs_status $status
fish "$_root/scripts/lint/no-implicit-alloc.fish" "$_root/tmp/no-implicit-alloc.ndjson" >/dev/null 2>&1
set noimplicitalloc_status $status
fish "$_root/scripts/lint/no-materialize.fish" "$_root/tmp/no-materialize.ndjson" >/dev/null 2>&1
set nomaterialize_status $status

# (r) TODO-precedence: drop marker-acknowledged Rust findings and emit the
#     merged view to tmp/lint-findings.ndjson. Fails while anything remains.
set -l precedence_status 1
fish "$_root/scripts/lint/todo-precedence.fish"
set precedence_status $status

# Summary. Rust sub-lints (clippy, panic, non-rudzio, no-suppressions,
# no-banner-comments, no-fallbacks, module-only, no-new-get,
# no-static-lifetime, no-tokio-runtime, no-manual-fmt, no-std-io,
# no-std-fs, no-implicit-alloc, no-materialize) read PASS/TODO/FAIL from the precedence
# status file;
# non-Rust sub-lints use their own status vars. The
# finding list and per-rule counts come from the merged view
# tmp/lint-findings.ndjson.
set -l s_fmt FAIL
test $fmt_failed -eq 0; and set s_fmt PASS
set -l s_fishsyntax FAIL
test $fishsyntax_status -eq 0; and set s_fishsyntax PASS
set -l s_nonfish FAIL
test $nonfish_status -eq 0; and set s_nonfish PASS
set -l s_yaml FAIL
test $yaml_status -eq 0; and set s_yaml PASS
set -l s_recipe FAIL
test $recipe_status -eq 0; and set s_recipe PASS
set -l s_wsdeps FAIL
test $wsdeps_status -eq 0; and set s_wsdeps PASS
set -l s_audit FAIL
test $audit_status -eq 0; and set s_audit PASS
set -l s_deny FAIL
test $deny_status -eq 0; and set s_deny PASS
set -l s_machete FAIL
test $machete_status -eq 0; and set s_machete PASS
set -l s_nixprov FAIL
test $nixprov_status -eq 0; and set s_nixprov PASS
set -l s_forbiddenword FAIL
test $forbiddenword_status -eq 0; and set s_forbiddenword PASS

echo
printf '%-16s %s\n' sub-lint status
printf '%-16s %s\n' format-check "$s_fmt"
printf '%-16s %s\n' fish-syntax "$s_fishsyntax"
printf '%-16s %s\n' non-fish "$s_nonfish"
printf '%-16s %s\n' yaml-lint "$s_yaml"
printf '%-16s %s\n' recipe-lint "$s_recipe"
printf '%-16s %s\n' workspace-deps "$s_wsdeps"
printf '%-16s %s\n' cargo-audit "$s_audit"
printf '%-16s %s\n' cargo-deny "$s_deny"
printf '%-16s %s\n' cargo-machete "$s_machete"
printf '%-16s %s\n' nix-provenance "$s_nixprov"
printf '%-16s %s\n' forbidden-word "$s_forbiddenword"
while read -l name st
    printf '%-16s %s\n' "$name" "$st"
end <"$_root/tmp/todo-precedence.status"

# Consolidated merged view.
set -l merged_count (wc -l <"$_root/tmp/lint-findings.ndjson" | string trim)
if test $merged_count -gt 0
    echo
    err "lint: $merged_count finding(s) remain (merged view)"
    jq -r '"\(.file):\(.line)  \(.ruleId)  \(.message)"' "$_root/tmp/lint-findings.ndjson"
    echo
    echo "Per-rule counts:"
    jq -r '.ruleId' "$_root/tmp/lint-findings.ndjson" | sort | uniq -c | sort -rn
end
echo

if test $fmt_failed -ne 0; or test $fishsyntax_status -ne 0; or test $nonfish_status -ne 0; or test $yaml_status -ne 0; or test $recipe_status -ne 0; or test $wsdeps_status -ne 0; or test $audit_status -ne 0; or test $deny_status -ne 0; or test $machete_status -ne 0; or test $nixprov_status -ne 0; or test $forbiddenword_status -ne 0; or test $precedence_status -ne 0
    err "lint failed"
    exit 1
end
ok "lint passed"
