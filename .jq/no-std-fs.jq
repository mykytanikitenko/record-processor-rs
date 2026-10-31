# no-std-fs classifier.
#
# Input: `jq -s` of an ast-grep `--json=stream` run with the
# `scripts/lint/no-std-fs/sgconfig.yml` config (single `severity: info` rule:
# std-fs-usage).
#
# Output: an array of finding objects `{file, line, column, ruleId, message}`,
# one per forbidden `std::fs` usage. Empty when clean.
#
# The rule: `std::fs` is forbidden everywhere. Unlike no-std-io there are no
# structural exceptions — every filesystem access must go through async
# `compio::fs`, so nothing is permitted and the classifier is a straight
# rename + record-shaping pass.

. as $all
| $all
| map(select(.ruleId == "std-fs-usage"))
| map(
    {file: (.file // ""),
     line: (.range.start.line + 1),
     column: (.range.start.column + 1),
     ruleId: "no-std-fs",
     message: ("forbidden `" + (.text | sub("\\s+"; " ")) + "`; use `compio::fs` instead")}
  )
| sort_by(.file, .line, .column)
| unique
