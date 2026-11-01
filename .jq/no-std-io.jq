# no-std-io classifier.
#
# Input: `jq -s` of an ast-grep `--json=stream` run with the
# `scripts/lint/no-std-io/sgconfig.yml` config (three `severity: info` rules:
# std-io-usage, permitted-from-field, permitted-framer-result).
#
# Output: an array of finding objects `{file, line, column, ruleId, message}`,
# one per forbidden `std::io` / `SyncStream` usage. Empty when clean.
#
# The rule: `std::io` and `compio::io::compat::SyncStream`/`SyncStream` are
# forbidden everywhere, with exactly two structural exceptions (which are what
# compio's codec protocol forces on us):
#   - a `#[from] std::io::Error` field inside a thiserror enum variant, and
#   - a `std::io::Result<…>` return type on a fn inside an `impl … Framer … for
#     …` block.
# There are no path/file exclusions: the permission is purely structural. A
# `std-io-usage` finding whose byte range lies inside a permitted finding's byte
# range (same file) is dropped; everything else is reported.

. as $all
| ($all
   | map(select(.ruleId == "permitted-from-field" or .ruleId == "permitted-framer-result"))
   | map({file: (.file // ""), s: .range.byteOffset.start, e: .range.byteOffset.end})) as $permitted
| $all
| map(select(.ruleId == "std-io-usage"))
| map(
    . as $m
    | (.file // "") as $file
    | (.range.byteOffset.start) as $s
    | (.range.byteOffset.end) as $e
    | (if ($permitted | any(.file == $file and .s <= $s and $e <= .e)) then empty
       else
         {file: $file,
          line: (.range.start.line + 1),
          column: (.range.start.column + 1),
          ruleId: "no-std-io",
          message: ("forbidden `" + (.text | sub("\\s+"; " ")) + "`; only `#[from] std::io::Error` and a `std::io::Result` return type inside a `Framer` impl are permitted")}
       end)
  )
| sort_by(.file, .line, .column)
| unique
