#!/usr/bin/env fish
# Non-fish script linter for the record-processor workspace.
#
# The only scripting language permitted in this repository is fish. Any
# git-tracked file whose shebang resolves to a non-fish interpreter, whose
# extension is a known script language, or whose Justfile-family recipe
# shebangs / shell settings target a non-fish interpreter is a failure.
#
# Usage (run from the repo root):
#   fish scripts/lint/non-fish.fish [OUTPUT_NDJSON]
#
# Exit status:
#   0 — self-tests passed and no non-fish scripts were found.
#   1 — self-tests failed, or one or more non-fish scripts were found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")

cd "$repo_root"

set -g forbidden_ext_regex '\.(sh|bash|zsh|ksh|dash|csh|tcsh|py|pyw|js|mjs|cjs|ts|pl|pm|rb|lua|php|bat|cmd|tcl|awk|sed|nu|elv|xsh|jl)$'

set -l out "$argv[1]"
if test -z "$out"
    set out "$NON_FISH_LINT_OUTPUT"
end
if test -z "$out"
    set out "tmp/non-fish.ndjson"
end
mkdir -p (path dirname "$out")

# Accumulated findings: one compact JSON object string per entry.
set -g findings

function _record --argument-names path line message
    set -l json (jq -cn --arg f "$path" --argjson l "$line" --arg m "$message" \
        '{file:$f,line:$l,ruleId:"no-non-fish-script",message:$m}')
    set -g findings $findings $json
end

# Resolve a shebang line to its interpreter basename (e.g. "fish", "bash").
function _shebang_interpreter --argument-names line
    set -l trimmed (string trim "$line")
    if not string match -qr '^#!' "$trimmed"
        echo ""
        return
    end
    set -l rest (string trim (string sub -s 3 "$trimmed"))
    set -l tokens (string split -n -- ' ' "$rest")
    if test (count $tokens) -eq 0
        echo ""
        return
    end
    set -l interp $tokens[1]
    if test (string replace -r '.*/' '' -- "$interp") = env
        set -l i 2
        if test $i -le (count $tokens); and test "$tokens[$i]" = -S
            set i (math $i + 1)
        end
        if test $i -le (count $tokens)
            set interp $tokens[$i]
        else
            set interp env
        end
    end
    string replace -r '.*/' '' -- "$interp"
end

function _is_justfile --argument-names path
    set -l base (string replace -r '.*/' '' -- "$path")
    if test "$base" = Justfile; or test "$base" = justfile; or test "$base" = .justfile
        return 0
    end
    if string match -qr '\.just$|\.justfile$' -- "$path"
        return 0
    end
    return 1
end

# Rule 1: flag a file by forbidden extension or non-fish shebang.
function _rule1_check --argument-names path content
    if string match -qr -- "$forbidden_ext_regex" "$path"
        set -l ext (string replace -r '.*\.' '' -- "$path")
        _record "$path" 0 "file extension '.$ext' is a forbidden script language (only fish)"
    end
    set -l first_line (string split \n -- "$content")[1]
    if string match -qr -- '^#![/ ]' "$first_line"
        set -l interp (_shebang_interpreter "$first_line")
        if test -n "$interp"; and test "$interp" != fish
            _record "$path" 1 "shebang interpreter is '$interp', not fish"
        end
    end
end

# Rule 2: flag Justfile-family files whose recipe shebangs or shell settings
# target a non-fish interpreter, or which lack the fish `set shell` directive.
function _justfile_check --argument-names path content
    set -l has_set_shell 0
    set -l lineno 0
    for line in (string split \n -- "$content")
        set lineno (math $lineno + 1)
        if string match -qr -- '^\s*set\s+shell\s*:=' "$line"
            set has_set_shell 1
            if not string match -qr -- fish "$line"
                _record "$path" $lineno 'set shell points to a non-fish shell'
            end
        else if string match -qr -- '^\s*set\s+script-interpreter\s*:=' "$line"
            if not string match -qr -- fish "$line"
                _record "$path" $lineno 'set script-interpreter points to a non-fish interpreter'
            end
        end
        if string match -qr -- '^\s*#![/ ]' "$line"
            set -l interp (_shebang_interpreter "$line")
            if test -n "$interp"; and test "$interp" != fish
                _record "$path" $lineno "recipe shebang interpreter is '$interp', not fish"
            end
        end
    end
    if test $has_set_shell -eq 0
        _record "$path" 0 'missing `set shell := ["fish", "-c"]` declaration'
    end
end

# Read file content as a single string, never expanded into argv.
# mode "git" reads the index blob (sparse-checkout safe); "disk" reads the
# working tree.
function _read_content --argument-names mode path
    if test "$mode" = git
        git cat-file -p ":$path" | string collect
    else
        cat "$path" | string collect
    end
end

# Classify a single (path, content) pair. Shared by the real scan and the
# runtime self-test so both exercise identical detection logic.
function _classify --argument-names path content
    if _is_justfile "$path"
        _justfile_check "$path" "$content"
    else
        _rule1_check "$path" "$content"
    end
end

function _check_path --argument-names mode path
    set -l content (_read_content "$mode" "$path")
    _classify "$path" "$content"
end

# Self-test: generate should-flag / should-pass samples at runtime, run the
# classifier against them, assert expected results, and clean up on exit.
function _self_test
    set -g selftest_dir "tmp/lint-selftest-$fish_pid"
    set -l dir "$selftest_dir"
    mkdir -p "$dir/should-flag" "$dir/should-pass"

    function _selftest_cleanup --on-event fish_exit
        rm -rf "$selftest_dir"
    end

    printf '%s\n' '#!/bin/bash' 'echo hi' >"$dir/should-flag/bash-shebang.sh"
    printf '%s\n' '#!/usr/bin/env bash' 'set -euo pipefail' >"$dir/should-flag/env-bash"
    printf '%s\n' 'foo:' '    #!/usr/bin/env bash' '    echo hi' >"$dir/should-flag/justfile-bash-recipe.just"
    printf '%s\n' 'print("hi")' >"$dir/should-flag/python-ext.py"

    printf '%s\n' '#!/usr/bin/env -S fish -c' 'echo hi' >"$dir/should-pass/env-S-fish"
    printf '%s\n' 'set shell := ["fish", "-c"]' '' 'bar:' '    #!/usr/bin/env fish' '    echo hi' >"$dir/should-pass/justfile-fish.just"
    printf '%s\n' 'just some plain text' >"$dir/should-pass/plain.txt"
    printf '%s\n' '#![deny(warnings)]' 'fn main() {}' >"$dir/should-pass/rust-inner-attr.rs"

    for f in (path resolve "$dir/should-flag"/*)
        set -l before (count $findings)
        _check_path disk "$f"
        set -l after (count $findings)
        if test $after -eq $before
            echo "✗ non-fish-lint: self-test failed — should-flag sample produced no finding: "(basename "$f") >&2
            return 1
        end
    end
    for f in (path resolve "$dir/should-pass"/*)
        set -l before (count $findings)
        _check_path disk "$f"
        set -l after (count $findings)
        if test $after -ne $before
            echo "✗ non-fish-lint: self-test failed — should-pass sample produced a finding: "(basename "$f") >&2
            return 1
        end
    end
    set -g findings
    echo "✓ non-fish-lint: self-tests passed"
    return 0
end

# 1. Self-test before scanning — a broken detector must fail loudly.
echo "ℹ non-fish-lint: running self-tests..."
if not _self_test
    exit 1
end

# 2. Scan every git-tracked file (index content, so sparse checkouts are
#    covered). No path-based exemptions.
echo "ℹ non-fish-lint: scanning tracked files..."
for path in (git ls-files --cached)
    _check_path git "$path"
end

# 3. Write ndjson output (empty file when clean).
printf '%s\n' $findings >"$out"

# 4. Human-readable diagnostics + summary.
set -l total (count $findings)
if test $total -gt 0
    echo "✗ non-fish-lint: $total non-fish script finding(s):"
    echo
    printf '%s\n' $findings | jq -r '"\(.file):\(.line)  no-non-fish-script  \(.message)"'
    echo
else
    echo "✓ non-fish-lint: no non-fish scripts found"
end
echo "ℹ non-fish-lint: JSON stream → $out"

if test $total -gt 0
    echo "✗ non-fish-lint: non-fish scripts found — failing lint" >&2
    exit 1
end
exit 0
