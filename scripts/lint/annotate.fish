#!/usr/bin/env fish
# annotate.fish — insert `// TODO(lint/<rule>): <message>` markers above every
# Rust lint finding so violations are visible in-place.
#
# Consumes the ndjson/clippy output already produced by the sub-lints (which
# must run first) and mutates Rust sources under crates/ and src/ only. Idempotent: a
# second run produces no diff, because a marker already sitting in the comment
# block directly above a finding is left untouched.
#
# Marker format: `// TODO(lint/<rule-id>): <first line of message>`, indented
# to match the finding line. Clippy/rustc rule ids are `clippy::<lint>` or the
# bare rustc lint name. Findings from the `todo-comments` rule are skipped so
# annotation never self-loops.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
cd "$repo_root"

# 1. Normalize every finding source into `{file, line, ruleId, message}`
#    records (line is 1-based). ast-grep `--json=stream` reports 0-based
#    `range.start.line`; the custom ndjson records already carry a 1-based
#    `line`; clippy structured JSON carries a 1-based `line_start`.
set -l findings_raw (mktemp)
for src in tmp/panic.ndjson tmp/non-rudzio.ndjson tmp/no-suppressions.ndjson tmp/no-banner-comments.ndjson tmp/no-fallbacks.ndjson tmp/module-only.ndjson tmp/no-new-get.ndjson tmp/no-static-lifetime.ndjson
    if test -s "$src"
        jq -c 'if has("range") then {file, line:(.range.start.line + 1), ruleId, message} else {file, line, ruleId, message} end' "$src" >>"$findings_raw"
    end
end
if test -s tmp/clippy.json
    jq -c '.[] | {file, line:.line_start, ruleId:(.code // "rustc"), message}' tmp/clippy.json >>"$findings_raw"
end

# 2. Keep only Rust files under crates/ and src/, drop the todo-comments rule, dedup
#    identical (file, line, ruleId) records, and sort by (file asc, line desc,
#    ruleId asc) so a file's insertions are applied bottom-up.
set -l findings (jq -sc 'map(select(.file | (startswith("crates/") or startswith("src/"))) | select(.file | endswith(".rs")) | select(.ruleId != "todo-comments")) | unique | sort_by(.file, -(.line), .ruleId) | .[]' "$findings_raw")
rm -f "$findings_raw"

if test (count $findings) -eq 0
    exit 0
end

# 3. Group findings per file (already sorted line-descending) and annotate
#    each file once. `current_file`/`current_findings`/`annotated_any` are
#    global so the `_flush_file` function can read/write them.
set -g current_file ""
set -g current_findings
set -g annotated_any 0

# Self-test the marker placement + idempotency logic (regression for the
# 1-indexed line-insertion off-by-one).
function _self_test
    set -g selftest_dir "tmp/annotate-selftest-$fish_pid"
    mkdir -p "$selftest_dir"
    set -l f "$selftest_dir/t.rs"
    printf '%s\n' 'fn main() {' '    let _ = r;' '}' >"$f"

    # Finding at line 2 (`let _ = r;`) → marker must land directly above it.
    set -g current_findings (jq -cn '{file:"'"$f"'",line:2,ruleId:"error-source-lost",message:"test"}')
    set -g annotated_any 0
    _flush_file "$f"
    set -l lines
    while read -l ln
        set -a lines $ln
    end <"$f"
    if not string match -q '*TODO(lint/error-source-lost)*' -- "$lines[2]"
        echo "✗ annotate: self-test failed — marker not directly above finding" >&2
        rm -rf "$selftest_dir"
        return 1
    end
    if not string match -q '    let _ = r;' -- "$lines[3]"
        echo "✗ annotate: self-test failed — finding not shifted to line below marker" >&2
        rm -rf "$selftest_dir"
        return 1
    end

    # Idempotency with a fresh finding line (line 3 after the shift).
    set -g current_findings (jq -cn '{file:"'"$f"'",line:3,ruleId:"error-source-lost",message:"test"}')
    _flush_file "$f"
    set -l lines2
    while read -l ln
        set -a lines2 $ln
    end <"$f"
    set -l before_text (string collect -- $lines)
    set -l after_text (string collect -- $lines2)
    test "$after_text" != "$before_text"
    set -l rc $status
    if test $rc -eq 0
        echo "✗ annotate: self-test failed — second pass not idempotent" >&2
        rm -rf "$selftest_dir"
        return 1
    else if test $rc -ge 2
        echo "✗ annotate: self-test errored" >&2
        rm -rf "$selftest_dir"
        return 1
    end

    rm -rf "$selftest_dir"
    set -g current_findings
    set -g annotated_any 0
    echo "✓ annotate: self-tests passed"
    return 0
end

function _flush_file --argument-names path
    set -l inserted 0
    # Read the file into a list of lines (0-indexed).
    set -l file_lines
    while read -l ln
        set -a file_lines $ln
    end <"$path"

    # current_findings holds the file's findings, line-descending. Insert each
    # missing marker above its line; walk up the contiguous comment block for
    # idempotency.
    for f in $current_findings
        set -l line (printf '%s' "$f" | jq -r '.line')
        set -l rule (printf '%s' "$f" | jq -r '.ruleId')
        set -l msg (printf '%s' "$f" | jq -r '.message' | string split \n)[1]

        # Idempotency: scan the contiguous `//` comment block directly above
        # the finding line for an existing marker for this rule. `file_lines`
        # is 1-indexed; the line directly above a finding at `line` is
        # `line - 1`.
        set -l idx (math "$line - 1")
        set -l seen 0
        while test $idx -ge 1
            set -l above (string trim -- "$file_lines[$idx]")
            if not string match -q '//*' -- "$above"
                break
            end
            if string match -q "*TODO(lint/$rule)*" -- "$above"
                set seen 1
                break
            end
            set idx (math "$idx - 1")
        end
        if test $seen -eq 1
            continue
        end

        # Indentation = leading whitespace of the finding's own line; marker
        # goes directly above it (insert at 1-based `line`).
        set -l target (math "$line")
        set -l indent ""
        if test $target -le (count $file_lines)
            set indent (string match -r '^\s*' -- "$file_lines[$target]")
        end
        set -l marker "$indent// TODO(lint/$rule): $msg"

        # Insert before the target line (1-indexed).
        if test $target -eq 1
            set file_lines $marker $file_lines
        else
            set file_lines $file_lines[1..(math "$target - 1")] $marker $file_lines[$target..-1]
        end
        set inserted 1
    end

    if test $inserted -eq 1
        set -g annotated_any 1
        printf '%s\n' $file_lines >"$path"
    end
    set -g current_findings
end

_self_test
or exit 1

for f in $findings
    set -l file (printf '%s' "$f" | jq -r '.file')
    if test "$file" != "$current_file"
        if test -n "$current_file"
            _flush_file "$current_file"
        end
        set -g current_file "$file"
        set -g current_findings
    end
    set -ga current_findings "$f"
end
if test -n "$current_file"
    _flush_file "$current_file"
end

# 4. Normalize indentation of the freshly inserted markers. rustfmt reindents
#    comment lines that sit inside a parameter list or before a closing brace,
#    so run it only when something was inserted. The subsequent re-scan in
#    scripts/lint.fish then reads the formatted line numbers.
if test $annotated_any -eq 1
    cargo fmt --all
end

exit 0
