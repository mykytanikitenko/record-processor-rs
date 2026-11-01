# no-global-allocator classifier.
#
# Input: `jq -s` of an ast-grep `--json=stream` run with the
# `scripts/lint/no-global-allocator/sgconfig.yml` config (one `severity: info`
# rule: global-allocator).
#
# Output: an array of finding objects `{file, line, column, ruleId, message}`,
# one per concrete `Global` allocator naming in a library crate. Empty when
# clean.
#
# The rule: library crates (storage, service, transport, domain) must thread
# `A: Allocator` generics; only the CLI composition root may name a concrete
# allocator. Scope is crate-directory based: `crates/cli`, `src/`, and
# `tests/` files are excluded.

def is_library_crate:
  test("(^|/)crates/(storage|service|transport|domain)/");
def is_test_file:
  test("(^|/)tests/");

. as $all
| $all
| map(select(.ruleId == "global-allocator"))
| map(
    . as $m
    | (.file // "") as $file
    | (if ($file | is_library_crate) and (($file | is_test_file) | not)
       then
         {file: $file,
          line: (.range.start.line + 1),
          column: (.range.start.column + 1),
          ruleId: "no-global-allocator",
          message: (.message)}
       else empty end)
  )
| sort_by(.file, .line, .column)
| unique
