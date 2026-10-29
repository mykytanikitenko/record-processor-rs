#!/usr/bin/env fish
# todo-precedence.fish — apply TODO marker precedence over the merged Rust
# findings and emit the final consolidated view.
#
# A Rust finding (file, line L, rule R) whose comment block directly above L
# contains `// TODO(lint/R)` is acknowledged in-place: the original finding is
# dropped and is surfaced instead by the todo-comments rule (which flags the
# marker comment itself). Findings without a marker are kept. Raw sub-lint
# ndjson files are left untouched; the deduplicated merged view is written to
# tmp/lint-findings.ndjson.
#
# Prints one `sub-lint status` line per Rust sub-lint:
#   PASS — no findings; TODO — all findings absorbed; FAIL — some remain.
# Exit 1 while any finding (kept or todo-comments) remains.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
cd "$repo_root"

# Source -> ndjson file (Rust-finding sources).
set -l sources panic tmp/panic.ndjson
set -a sources non-rudzio tmp/non-rudzio.ndjson
set -a sources no-suppressions tmp/no-suppressions.ndjson
set -a sources no-banner-comments tmp/no-banner-comments.ndjson
set -a sources no-fallbacks tmp/no-fallbacks.ndjson
set -a sources module-only tmp/module-only.ndjson
set -a sources clippy tmp/clippy.json

# Normalized findings: {source, file, line, ruleId, message}. ast-grep
# `--json=stream` reports 0-based `range.start.line`; custom ndjson records
# carry a 1-based `line`; clippy structured JSON carries 1-based `line_start`.
set -l raw (mktemp)
set -l i 1
while test $i -le (count $sources)
    set -l name $sources[$i]
    set -l path $sources[(math "$i + 1")]
    if test -s "$path"
        if test "$name" = clippy
            jq -c --arg s "$name" '.[] | {source:$s, file, line:.line_start, ruleId:(.code // "rustc"), message}' "$path" >>"$raw"
        else
            jq -c --arg s "$name" 'if has("range") then {source:$s, file, line:(.range.start.line + 1), ruleId, message} else {source:$s, file, line, ruleId, message} end' "$path" >>"$raw"
        end
    end
    set i (math "$i + 2")
end

# Rust findings are absorbable; everything else is reported unchanged.
set -l rust_findings (jq -c 'select(.file | (startswith("crates/") or startswith("src/"))) | select(.file | endswith(".rs"))' "$raw" | sort -u)
set -l nonrust_findings (jq -c 'select((.file | (startswith("crates/") or startswith("src/"))) | not)' "$raw" | sort -u)
rm -f "$raw"

# Shared state read/written by `_flush_precedence` (global so the function can
# see it). `kept` accumulates non-absorbed Rust findings.
set -g kept (mktemp)
set -g current_file ""
set -g current_findings
set -g file_lines

function _flush_precedence --argument-names path
    for f in $current_findings
        set -l line (printf '%s' "$f" | jq -r '.line')
        set -l rule (printf '%s' "$f" | jq -r '.ruleId')
        set -l idx (math "$line - 1")
        set -l absorbed 0
        while test $idx -ge 1
            set -l above (string trim -- "$file_lines[$idx]")
            if not string match -q '//*' -- "$above"
                break
            end
            if string match -q "*TODO(lint/$rule)*" -- "$above"
                set absorbed 1
                break
            end
            set idx (math "$idx - 1")
        end
        if test $absorbed -eq 0
            printf '%s\n' "$f" >>"$kept"
        end
    end
    set -g current_findings
end

for f in $rust_findings
    set -l file (printf '%s' "$f" | jq -r '.file')
    if test "$file" != "$current_file"
        if test -n "$current_file"
            _flush_precedence "$current_file"
        end
        set -g current_file "$file"
        set -g current_findings
        set -g file_lines
        while read -l ln
            set -ga file_lines $ln
        end <"$file"
    end
    set -ga current_findings "$f"
end
if test -n "$current_file"
    _flush_precedence "$current_file"
end

# Merged view: kept Rust findings + non-Rust findings.
set -l merged (mktemp)
cat "$kept" >>"$merged"
if test (count $nonrust_findings) -gt 0
    printf '%s\n' $nonrust_findings >>"$merged"
end
rm -f "$kept"
sort -u "$merged" >"tmp/lint-findings.ndjson"
rm -f "$merged"

# Per-source status + overall. Write machine-readable `name<TAB>status` lines to
# tmp/todo-precedence.status for the orchestrator's summary table.
set -l status_file "tmp/todo-precedence.status"
printf '' >"$status_file"
set -l had_remaining 0
set -l i 1
while test $i -le (count $sources)
    set -l name $sources[$i]
    set -l total (printf '%s\n' $rust_findings | jq -c --arg s "$name" 'select(.source == $s)' | wc -l | string trim)
    set -l kept_count (jq -c --arg s "$name" 'select(.source == $s)' "tmp/lint-findings.ndjson" | wc -l | string trim)
    set -l st PASS
    if test $total -gt 0
        if test $kept_count -gt 0
            set st FAIL
            set had_remaining 1
        else
            set st TODO
        end
    end
    printf '%-16s %s\n' "$name" "$st"
    printf '%s\t%s\n' "$name" "$st" >>"$status_file"
    set i (math "$i + 2")
end

set -l remaining (wc -l <"tmp/lint-findings.ndjson" | string trim)
if test $remaining -gt 0
    set had_remaining 1
end
if test $had_remaining -eq 1
    exit 1
end
exit 0
