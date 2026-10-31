# no-to-string classifier.
#
# Input: `jq -s` of an ast-grep `--json=stream` run with the
# `scripts/lint/no-to-string/sgconfig.yml` config (single `severity: info`
# rule: to-string-usage).
#
# Output: an array of finding objects `{file, line, column, ruleId, message}`,
# one per forbidden `.to_string()`/`.to_owned()` call. Empty when clean.
#
# The rule: owned-string conversion is forbidden — the record-processor
# pipeline is zero-copy, so every `.to_string()`/`.to_owned()` must be
# replaced by a `format!`, `String::from`, a `&str` borrow, or `Into::into`.

. as $all
| $all
| map(select(.ruleId == "to-string-usage"))
| map(
    {file: (.file // ""),
     line: (.range.start.line + 1),
     column: (.range.start.column + 1),
     ruleId: "no-to-string",
     message: ("owned-string conversion via `" + (.text | sub("\\s+"; " ")) + "`; use `format!`, `String::from`, a `&str` borrow, or `Into::into`")}
  )
| sort_by(.file, .line, .column)
| unique
