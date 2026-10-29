# forbidden-word.fish — shared helpers for the forbidden-word lint and the
# commit-msg hook.
#
# Sourced (never executed directly) by scripts/lint/forbidden-word.fish and
# scripts/hooks/commit-msg. It wraps scripts/lib/forbidden-word.jq, the single
# source of truth for the normalizer and the match, so the commit-time check
# and the full content/history scan can never drift apart.
#
# The target word is never written literally anywhere in this repository; it
# is assembled from ASCII code points inside the jq filter.

set -g _forbidden_word_jq (path resolve (status dirname)/forbidden-word.jq)

# Emit one JSON record per match in $text (empty output when clean), e.g.
#   {"line":3,"match":"forward"}
function _forbidden_word_matches --argument-names text
    printf '%s' "$text" | jq -Rs -c -f "$_forbidden_word_jq"
end

# Return 0 when $text contains the forbidden word (forward or reverse), else 1.
function _forbidden_word_found --argument-names text
    set -l hits (_forbidden_word_matches "$text")
    test (count $hits) -gt 0
end
