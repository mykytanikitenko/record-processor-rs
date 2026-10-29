# no-materialize classifier.
#
# Input: `jq -s` of an ast-grep `--json=stream` run with the
# `scripts/lint/no-materialize/sgconfig.yml` config (two `severity: info`
# rules: heap-collection, test-module), plus a `--argjson domains` array of
# domain type names.
#
# Output: an array of finding objects `{file, line, column, ruleId, message}`,
# one per heap collection holding a domain type, in non-test code.
#
# The rule: domain types (the `pub struct`/`pub enum` names in
# `crates/domain/src`, discovered at lint time) must not be materialized into
# heap collections — `Vec<D>`, `VecDeque<D>`, `Box<[D]>`. The pipeline streams
# domain records lazily through memory-mapped buffers instead of accumulating
# them on the heap, so any such collection is a code smell. Test code is exempt
# structurally (files under `tests/`, items inside `#[cfg(test)]` modules).

def is_real_mod:
  .text | test("^(pub([[:space:]]*\\([^)]*\\))?[[:space:]]+)?mod[[:space:]]");

def last_segment:
  sub("^.*::(?=[A-Z])"; "");

# The type argument of a `Vec<T>`/`VecDeque<T>`/`Box<[T]>` generic type.
def type_arg:
  if test("^Box\\s*<\\s*\\[") then
    sub("^Box\\s*<\\s*\\["; "") | sub("\\]\\s*>\\s*$"; "")
  else
    sub("^(?:Vec|VecDeque)\\s*<"; "") | sub(">\\s*$"; "")
  end;

. as $all
| ($domains // []) as $domains
| ($all | map(select(.ruleId == "test-module") | select(is_real_mod))
        | map({file: (.file // ""), s: .range.byteOffset.start, e: .range.byteOffset.end})) as $testmods
| $all
| map(select(.ruleId == "heap-collection"))
| map(
    . as $m
    | ((.text // "") | type_arg | last_segment) as $ty
    | (.file // "") as $file
    | (.range.byteOffset.start) as $s
    | (if ($file | test("(^|/)tests/")) then empty
       elif ($testmods | any(.file == $file and .s <= $s and $s < .e)) then empty
       elif ($domains | index($ty)) then
         {file: $file,
          line: (.range.start.line + 1),
          column: (.range.start.column + 1),
          ruleId: "no-materialize",
          message: ("materializes domain type `" + $ty + "` into a heap collection (`" + (.text | sub("\\s+"; " ")) + "`); stream domain records lazily instead")}
       else empty
       end)
  )
| sort_by(.file, .line, .column)
| unique
