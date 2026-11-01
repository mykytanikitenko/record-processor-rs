# no-implicit-alloc classifier.
#
# Input: `jq -s` of an ast-grep `--json=stream` run with the
# `scripts/lint/no-implicit-alloc/sgconfig.yml` config (two `severity: info`
# rules: alloc-pattern, test-module).
#
# Output: an array of finding objects `{file, line, column, ruleId, message}`,
# one per forbidden heap allocation in non-test code. Empty when clean.
#
# The rule: implicit heap allocation is forbidden in production code — the
# record-processor pipeline logs to a memory-mapped append-only file instead of
# the heap, so every `Vec::new`/`vec![]`/`Box::new`/`String::new`/`format!`/
# `.collect`/… must be replaced by a memory-mapped, reused-buffer, or borrowed
# alternative. Test code is exempt (structurally): files under `tests/`, and
# items inside `#[cfg(test)]` modules, are dropped by byte-range subtraction,
# exactly as the shared-error-types rule excludes test code.

def is_real_mod:
  .text | test("^(pub([[:space:]]*\\([^)]*\\))?[[:space:]]+)?mod[[:space:]]");

. as $all
| ($all | map(select(.ruleId == "test-module") | select(is_real_mod))
        | map({file: (.file // ""), s: .range.byteOffset.start, e: .range.byteOffset.end})) as $testmods
| $all
| map(select(.ruleId == "alloc-pattern"))
| map(
    . as $m
    | (.file // "") as $file
    | (.range.byteOffset.start) as $s
    | (if ($file | test("(^|/)tests/")) then empty
       elif ($testmods | any(.file == $file and .s <= $s and $s < .e)) then empty
       else
         {file: $file,
          line: (.range.start.line + 1),
          column: (.range.start.column + 1),
          ruleId: "no-implicit-alloc",
          message: ("implicit heap allocation via `" + (.text | sub("\\s+"; " ")) + "`; use a memory-mapped, reused-buffer, or borrowed alternative")}
       end)
  )
| sort_by(.file, .line, .column)
| unique
