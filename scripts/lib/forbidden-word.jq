# forbidden-word.jq — normalization + detection for the forbidden brand word.
#
# This file is a jq filter consumed by scripts/lib/forbidden-word.fish and,
# through it, by scripts/lint/forbidden-word.fish and scripts/hooks/commit-msg.
# It is the single source of truth for both the normalizer and the match, so
# the commit-time check and the full content/history scan can never drift
# apart.
#
# The target word is NEVER written literally here, nor is any look-alike
# spelling of it. It is assembled from ASCII code points at load time, and
# every non-ASCII homoglyph it must recognise is expressed as a \uXXXX escape
# rather than as a literal glyph.
#
# Input (via `jq -Rs`): the raw text, slurped into an array of input lines.
# Output: one JSON object per match, e.g. {"line":3,"match":"forward"}.
# "line" is the 1-based source line of the first matched letter (letters are
# collapsed across the WHOLE input, so a word spread across several lines is
# still caught and attributed to the line of its first letter). "match" is
# "forward" when the text folds to the target word and "reverse" when it folds
# to the word reversed.

# The target word, built from code points (k=107 r=114 a=97 k=107 e=101 n=110).
def target: [107,114,97,107,101,110] | implode;
# The same word reversed, so reverse-order obfuscation is caught too.
def rtarget: target | explode | reverse | implode;

# Fold look-alike glyphs and leetspeak to plain ASCII. Multi-character
# leetspeak runs first so `|<` -> k is not clobbered by any later rule.
def fold:
  # multi-char leetspeak
  gsub("\\|<"; "k")          # |< -> k
  | gsub("\\|2"; "r")        # |2 -> r
  # single-char leetspeak
  | gsub("[4@\u2202]"; "a")  # 4 @ U+2202 -> a
  | gsub("[3\u20AC]"; "e")   # 3 U+20AC -> e
  | gsub("\u0138"; "k")      # U+0138 (latin kra) -> k
  | gsub("\u00AE"; "r")      # U+00AE (registered sign) -> r
  # Cyrillic homoglyphs (lower and upper)
  | gsub("[\u0430\u0410]"; "a")
  | gsub("[\u0435\u0415]"; "e")
  | gsub("[\u043A\u041A]"; "k")
  | gsub("[\u043D\u041D]"; "n")
  | gsub("[\u0440\u0420]"; "r")
  # Greek homoglyphs (lower and upper)
  | gsub("[\u03B1\u0391]"; "a")
  | gsub("[\u03B5\u0395]"; "e")
  | gsub("[\u03BA\u039A]"; "k")
  | gsub("[\u03BD\u039D]"; "n")
  | gsub("[\u03C1\u03A1]"; "r")
  # Fullwidth letters (lower and upper)
  | gsub("[\uFF41\uFF21]"; "a")
  | gsub("[\uFF45\uFF25]"; "e")
  | gsub("[\uFF4B\uFF2B]"; "k")
  | gsub("[\uFF4E\uFF2E]"; "n")
  | gsub("[\uFF52\uFF32]"; "r");

# A string's surviving letters, as an array of single-character strings, after
# folding look-alikes, lowercasing ASCII, and dropping every non-letter.
# Zero-width and format chars (U+200B..U+200F, U+2060, U+FEFF, soft hyphen
# U+00AD) are not in [a-z], so the final collapse removes them without a
# dedicated pass.
def letters: fold | ascii_downcase | gsub("[^a-z]"; "") | explode | map([.] | implode);

# Collapse the whole input (an array of lines) into a flat letter stream with
# per-letter source-line attribution.
def collapse:
  . as $lines
  | reduce range(0; ($lines | length)) as $i (
      {text: "", line_of: []};
      ($lines[$i] | letters) as $ls
      | {text: (.text + ($ls | join(""))),
         line_of: (.line_of + ($ls | map($i + 1)))}
    );

# Emit one {line, match} record per occurrence of the target (forward or
# reverse) in the collapsed stream.
def scan:
  collapse as $c
  | ($c.text) as $t
  | (target | length) as $n
  | ([range(0; ($t | length) - $n + 1) as $i
      | select($t[$i:($i + $n)] == target)
      | {line: ($c.line_of[$i]), match: "forward"}]
     + [range(0; ($t | length) - $n + 1) as $i
        | select($t[$i:($i + $n)] == rtarget)
        | {line: ($c.line_of[$i]), match: "reverse"}])
  | unique
  | .[];

# Main: `-Rs` delivers the raw text as a single string, so split it into lines
# before scanning.
split("\n") | scan
