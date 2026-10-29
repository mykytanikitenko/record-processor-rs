#!/usr/bin/env fish
# no-banner-comments linter: forbid decorative separator-banner comments
# across the workspace.
#
# A banner comment is a comment line whose remaining content (after the
# language's comment marker) is either (a) >= 4 consecutive repeated chars
# from `= - * # ~ _ + /` with nothing else but whitespace and an optional
# closing `*/` / `-->`, or (b) text sandwiched between >= 4 such chars on
# both sides (e.g. `// ===== Foo =====`). See `_is_banner_line` for the
# authoritative classifier shared by the real scan and the runtime
# self-tests.
#
# Only *comment* lines are considered. YAML `---` document markers, Markdown
# `---`/`***` hrules, TOML `[table]` headers, shebangs, and Rust `#![attr]`
# inner attributes are not comments (or, for shebangs, not banner text) and
# are never flagged. Markdown is scanned only for HTML comments (`<!-- -->`).
#
# Usage (run from the repo root):
#   fish scripts/lint/no-banner-comments.fish [OUTPUT_NDJSON]
#
# Exit status:
#   0 — self-tests passed and no banner comments were found.
#   1 — self-tests failed, or one or more banner comments were found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")

cd "$repo_root"

set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/no-banner-comments.ndjson"
end
mkdir -p (path dirname "$out")

# Banner-shape regexes applied to the comment remainder (after the marker).
# `\1{3,}` enforces >= 4 repeats of the SAME banner char. `set -g` so the
# `_is_banner_line` function can read them (fish functions cannot see
# script-local `set -l` variables).
set -g _pure_re '^\s*([=\-*#~_+/])\1{3,}\s*(?:\*/\s*|-->\s*)?$'
set -g _sand_re '^\s*([=\-*#~_+/])\1{3,}\s+.+\s+\1{3,}\s*(?:\*/\s*|-->\s*)?$'

# Accumulated findings: one compact JSON object string per entry.
set -g findings

function _record --argument-names path line column message
    set -l json (jq -cn --arg f "$path" --argjson l "$line" --argjson c "$column" --arg m "$message" \
        '{file:$f,line:$l,column:$c,ruleId:"no-banner-comments",message:$m}')
    set -g findings $findings $json
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

# Emit the space-separated comment markers for a path (longest first), or
# empty when the file type is unrecognized. `content` is only consulted for
# extensionless files (fish-shebang detection).
function _markers_for --argument-names path content
    if string match -qr '\.rs$' -- "$path"
        echo "//! /// // /* *"
    else if string match -qr '\.nix$' -- "$path"
        echo "/* * #"
    else if string match -qr '\.(toml|yml|yaml|fish)$' -- "$path"
        echo "#"
    else if string match -qr '\.md$' -- "$path"
        echo "<!--"
    else if _is_justfile "$path"
        echo "#"
    else if not string match -qr '\.[^/]+$' -- "$path"
        set -l first (string split -n \n -- "$content")[1]
        if string match -qr '^#!.*\bfish\b' -- "$first"
            echo "#"
        else
            echo ""
        end
    else
        echo ""
    end
end

# Classify a single line as a banner comment. Returns 0 when `$line` is a
# banner in a language using `$markers`, else 1.
function _is_banner_line --argument-names markers line
    set -l trimmed (string trim --left -- "$line")
    if test -z "$trimmed"
        return 1
    end
    set -l remainder ""
    set -l found 0
    for marker in (string split ' ' -- "$markers")
        set -l mlen (string length -- "$marker")
        set -l prefix (string sub -l $mlen -- "$trimmed")
        if test "$prefix" = "$marker"
            set remainder (string sub -s (math $mlen + 1) -- "$trimmed")
            set found 1
            break
        end
    end
    if test $found -eq 0
        return 1
    end
    if string match -q -r -- $_pure_re "$remainder"
        return 0
    end
    if string match -q -r -- $_sand_re "$remainder"
        return 0
    end
    return 1
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
    set -l markers (_markers_for "$path" "$content")
    if test -z "$markers"
        return
    end
    set -l lineno 0
    for line in (string split \n -- "$content")
        set lineno (math $lineno + 1)
        if _is_banner_line "$markers" "$line"
            _record "$path" $lineno 1 "separator banner comment"
        end
    end
end

function _check_path --argument-names mode path
    set -l content (_read_content "$mode" "$path")
    _classify "$path" "$content"
end

# Self-test: generate should-flag / should-pass samples at runtime, run the
# classifier against them, assert expected results, and clean up on exit.
function _self_test
    set -g selftest_dir "tmp/banner-selftest-$fish_pid"
    set -l dir "$selftest_dir"
    mkdir -p "$dir/should-flag" "$dir/should-pass"

    function _selftest_cleanup --on-event fish_exit
        rm -rf "$selftest_dir"
    end

    printf '%s\n' '// ===========================================================================' >"$dir/should-flag/rust-line.rs"
    printf '%s\n' '// ===== Foo =====' >"$dir/should-flag/rust-sandwiched.rs"
    printf '%s\n' '/* =========================================================================== */' >"$dir/should-flag/rust-block.rs"
    printf '%s\n' '//! ===========================================================================' >"$dir/should-flag/rust-inner-doc.rs"
    printf '%s\n' '# ===========================================================================' >"$dir/should-flag/toml.toml"
    printf '%s\n' '# ===========================================================================' >"$dir/should-flag/nix.nix"
    printf '%s\n' '# ===========================================================================' >"$dir/should-flag/justfile.just"
    printf '%s\n' '<!-- =========================================================================== -->' >"$dir/should-flag/md.md"
    printf '%s\n' '# ===========================================================================' >"$dir/should-flag/fish.fish"

    printf '%s\n' --- >"$dir/should-pass/yaml-doc-marker.yml"
    printf '%s\n' --- '***' >"$dir/should-pass/md-hrule.md"
    printf '%s\n' ______ >"$dir/should-pass/md-underscore-hrule.md"
    printf '%s\n' '[workspace]' >"$dir/should-pass/toml-table.toml"
    printf '%s\n' '#!/usr/bin/env fish' 'echo hi' >"$dir/should-pass/shebang.fish"
    printf '%s\n' '#![allow(dead_code)]' 'fn main() {}' >"$dir/should-pass/rust-inner-attr.rs"
    printf '%s\n' '// Rust lints' >"$dir/should-pass/rust-title.rs"
    printf '%s\n' '// ---' >"$dir/should-pass/rust-short.rs"

    for f in (path resolve "$dir/should-flag"/*)
        set -l before (count $findings)
        _check_path disk "$f"
        set -l after (count $findings)
        if test $after -eq $before
            echo "✗ no-banner-comments: self-test failed — should-flag sample produced no finding: "(basename "$f") >&2
            return 1
        end
    end
    for f in (path resolve "$dir/should-pass"/*)
        set -l before (count $findings)
        _check_path disk "$f"
        set -l after (count $findings)
        if test $after -ne $before
            echo "✗ no-banner-comments: self-test failed — should-pass sample produced a finding: "(basename "$f") >&2
            return 1
        end
    end
    set -g findings
    echo "✓ no-banner-comments: self-tests passed"
    return 0
end

# 1. Self-test before scanning — a broken detector must fail loudly.
echo "ℹ no-banner-comments: running self-tests..."
if not _self_test
    exit 1
end

# 2. Scan every git-tracked file (index content, so sparse checkouts are
#    covered). No path-based exemptions.
echo "ℹ no-banner-comments: scanning tracked files..."
for path in (git ls-files --cached)
    _check_path git "$path"
end

# 3. Write ndjson output (empty file when clean).
printf '%s\n' $findings >"$out"

# 4. Human-readable diagnostics + summary.
set -l total (count $findings)
if test $total -gt 0
    echo "✗ no-banner-comments: $total banner comment finding(s):"
    echo
    printf '%s\n' $findings | jq -r '"\(.file):\(.line):\(.column)  no-banner-comments  \(.message)"'
    echo
else
    echo "✓ no-banner-comments: no banner comments found"
end
echo "ℹ no-banner-comments: JSON stream → $out"

if test $total -gt 0
    echo "✗ no-banner-comments: banner comments found — failing lint" >&2
    exit 1
end
exit 0
