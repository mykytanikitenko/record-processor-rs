#!/usr/bin/env fish
# forbidden-word linter: forbid a 6-letter brand name across the whole
# repository, its commit history, and its commit/author metadata.
#
# The target is a brand name the operator wants kept out of this
# brand-neutral repository. It is NEVER written literally here (or anywhere in
# this repo): it is assembled from ASCII code points inside
# scripts/lib/forbidden-word.jq, and every look-alike glyph the detector must
# recognise is expressed there as a \uXXXX escape. Detection is
# normalization-based, so case variants, zero-width/format separators,
# Cyrillic/Greek/fullwidth homoglyphs, leetspeak, arbitrary non-letter
# separators (including newlines), and the reversed word are all caught.
#
# Scope (no path or file exemptions, per operator rule):
#   1. Content scan — every git-tracked text file. Binary files are skipped
#      using git's own `git grep -I` text/binary detection.
#   2. History scan — commit message bodies, author names, and committer
#      names reachable from HEAD, plus the content of every blob ever
#      reachable from HEAD (so a file that once contained the word and was
#      later deleted is still caught).
#
# Findings are emitted as ndjson records: content findings carry
# `file: <path>, line: N`; history findings carry `file: <object SHA>,
# line: N` (the commit-message line, or 0 for a name field) with a `message`
# naming the field. `line` is the 1-based line of the first matched letter.
#
# Usage (run from the repo root):
#   fish scripts/lint/forbidden-word.fish [OUTPUT_NDJSON]
#
# Exit status:
#   0 — self-tests passed and no occurrence was found.
#   1 — self-tests failed, or one or more occurrences were found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")

cd "$repo_root"

source "$repo_root/scripts/lib/colors.fish"
source "$repo_root/scripts/lib/forbidden-word.fish"

set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/forbidden-word.ndjson"
end
mkdir -p (path dirname "$out")

# Accumulated findings: one compact JSON object string per entry.
set -g findings

function _record --argument-names path line message
    set -l json (jq -cn --arg f "$path" --argjson l "$line" --arg m "$message" \
        '{file:$f,line:$l,ruleId:"forbidden-word",message:$m}')
    set -g findings $findings $json
end

# Record a finding for every match in a (possibly multi-line) text blob
# against a given key (a repo-relative path or an object SHA). `what` names
# the scanned field for the finding message.
function _record_hits --argument-names key text what
    set -l hits (_forbidden_word_matches "$text")
    for h in $hits
        set -l ln (printf '%s' "$h" | jq -r '.line')
        set -l dir (printf '%s' "$h" | jq -r '.match')
        _record "$key" "$ln" "$what contains the forbidden word ($dir)"
    end
end

# git's own binary detection: `git grep -I` skips binary files. A non-empty
# text file matches the empty pattern (exit 0); a binary file is skipped
# (exit 1). Empty files are text with nothing to match, so return not-binary.
function _git_is_binary --argument-names path
    if test (git cat-file -s ":$path") -eq 0
        return 1
    end
    git grep -I -q -e '' -- "$path"
    set -l rc $status
    test $rc -ne 0
end

# --- self-test -----------------------------------------------------------
function _self_test
    set -g selftest_dir "tmp/forbidden-word-selftest-$fish_pid"
    set -l dir "$selftest_dir"
    mkdir -p "$dir/should-flag" "$dir/should-pass"

    function _selftest_cleanup --on-event fish_exit
        rm -rf "$selftest_dir"
    end

    # The target word and its obfuscations, built at runtime from code points —
    # never written literally.
    set -l word (jq -nr '[107,114,97,107,101,110] | implode')
    set -l rev (jq -nr '[107,114,97,107,101,110] | reverse | implode')
    set -l cyr (jq -nr '[1082,1088,1072,1082,1077,1085] | implode')
    set -l fwid (jq -nr '[65355,65362,65345,65355,65349,65358] | implode')
    set -l zwj (jq -nr '[107,8203,114,8203,97,8203,107,8203,101,8203,110] | implode')

    set -l dots (string split '' -- "$word" | string join '.')
    set -l spaces (string split '' -- "$word" | string join ' ')
    set -l newlines (string split '' -- "$word" | string join \n)
    set -l upper (string upper -- "$word")
    set -l mixed (string sub -l 1 -- "$upper")(string sub -s 2 -- "$word")
    set -l leet (string replace -a a 4 -- "$word" | string replace -a e 3 | string replace -a k '|<' | string replace -a r '|2')

    # should-flag: every obfuscation must be detected.
    printf '%s\n' "$word" >"$dir/should-flag/plain.txt"
    printf '%s\n' "$upper" >"$dir/should-flag/uppercase.txt"
    printf '%s\n' "$mixed" >"$dir/should-flag/mixed-case.txt"
    printf '%s\n' "$dots" >"$dir/should-flag/separated-dots.txt"
    printf '%s\n' "$spaces" >"$dir/should-flag/separated-spaces.txt"
    printf '%s\n' "$newlines" >"$dir/should-flag/separated-newlines.txt"
    printf '%s\n' "$leet" >"$dir/should-flag/leetspeak.txt"
    printf '%s\n' "$cyr" >"$dir/should-flag/cyrillic.txt"
    printf '%s\n' "$zwj" >"$dir/should-flag/zero-width.txt"
    printf '%s\n' "$fwid" >"$dir/should-flag/fullwidth.txt"
    printf '%s\n' "$rev" >"$dir/should-flag/reversed.txt"

    # should-pass: near-misses and negative controls must not be detected.
    printf '%s\n' track kraft brake rake taken kernel krak >"$dir/should-pass/negative.txt"
    printf '%s\n' 'this repository is brand-neutral; please rephrase' >"$dir/should-pass/english.txt"

    for f in (path resolve "$dir/should-flag"/*)
        set -l before (count $findings)
        _record_hits "$f" (cat "$f" | string collect) content
        set -l after (count $findings)
        if test $after -eq $before
            echo "✗ forbidden-word: self-test failed — should-flag sample produced no finding: "(basename "$f") >&2
            return 1
        end
    end
    for f in (path resolve "$dir/should-pass"/*)
        set -l before (count $findings)
        _record_hits "$f" (cat "$f" | string collect) content
        set -l after (count $findings)
        if test $after -ne $before
            echo "✗ forbidden-word: self-test failed — should-pass sample produced a finding: "(basename "$f") >&2
            return 1
        end
    end
    set -g findings
    echo "✓ forbidden-word: self-tests passed"
    return 0
end

# 1. Self-test before scanning — a broken detector must fail loudly.
echo "ℹ forbidden-word: running self-tests..."
if not _self_test
    exit 1
end

# 2. Content scan: every git-tracked file (binary skipped via git).
echo "ℹ forbidden-word: scanning tracked files..."
set -g tracked_binary 0
for path in (git ls-files --cached)
    if _git_is_binary "$path"
        set tracked_binary (math "$tracked_binary + 1")
        continue
    end
    _record_hits "$path" (git cat-file -p ":$path" | string collect) content
end
if test $tracked_binary -gt 0
    info "forbidden-word: $tracked_binary tracked binary file(s) skipped"
else
    info "forbidden-word: no tracked binary files"
end

# 3. History scan: commit messages + author/committer names + every blob.
echo "ℹ forbidden-word: scanning commit history..."
set -l hist_t0 (date +%s%N)
for sha in (git rev-list HEAD)
    set -l author (git log -1 --format=%an "$sha")
    set -l committer (git log -1 --format=%cn "$sha")
    if _forbidden_word_found "$author"
        _record "$sha" 0 "author name contains the forbidden word"
    end
    if _forbidden_word_found "$committer"
        _record "$sha" 0 "committer name contains the forbidden word"
    end
    _record_hits "$sha" (git log -1 --format=%B "$sha" | string collect) "commit message"
end
for entry in (git rev-list --objects HEAD)
    set -l blob (string split -f1 ' ' -- "$entry")[1]
    set -l path (string split -f2 ' ' -- "$entry")[2]
    if test -z "$path"
        continue
    end
    if test (git cat-file -t "$blob") != blob
        continue
    end
    _record_hits "$blob" (git cat-file blob "$blob" | string collect) "historical blob $path"
end
set -l hist_t1 (date +%s%N)
set -l hist_ms (math "($hist_t1 - $hist_t0) / 1000000")
info "forbidden-word: history scan took $hist_ms ms"

# 4. Write ndjson output (empty file when clean).
printf '%s\n' $findings >"$out"

# 5. Human-readable diagnostics + summary.
set -l total (count $findings)
if test $total -gt 0
    echo "✗ forbidden-word: $total occurrence(s):"
    echo
    printf '%s\n' $findings | jq -r '"\(.file):\(.line)  forbidden-word  \(.message)"'
    echo
else
    echo "✓ forbidden-word: no occurrences found"
end
echo "ℹ forbidden-word: JSON stream → $out"

if test $total -gt 0
    echo "✗ forbidden-word: occurrences found — failing lint" >&2
    exit 1
end
exit 0
