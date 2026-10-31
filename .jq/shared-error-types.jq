# shared-error-types classifier.
#
# Input: `jq -s` of an ast-grep `--json=stream` run over the workspace with the
# `scripts/lint/shared-error-types/sgconfig.yml` config (four `severity: info`
# extraction rules: error-slot-result, error-slot-assoc, result-alias,
# test-module).
#
# Output: an array of finding objects `{file, line, column, ruleId, message}`,
# one per site of a violation. Empty when the workspace is clean.
#
# The rule: every fallible fn/method owns its OWN error type. One error type may
# not be the error of two different functions. Wrapping a child error as a
# variant (`#[from]`/`#[source]`) is fine; reusing the same return error type is
# not. Two extra findings are emitted regardless of cross-file counting:
#   - `type Result<…> = …Result<…, E>` aliases that fix the error type defeat
#     the rule (the error is hidden behind the alias) and are forbidden.
#   - `Box<dyn Error>` / `anyhow::Error` error slots are shared-by-construction
#     and are forbidden outside test code.
#
# Error-slot definition:
#   (a) the second type argument `E` of `Result<T, E>` in the return type of a
#       fn/method (free fn, inherent method, trait method decl, trait impl
#       method);
#   (b) `type Error = E;` inside an impl block (TryFrom, FromStr, …).
#
# Normalization: `E` is reduced to its last path segment plus generics as
# written, so `crate::amount::AmountNewError` and `AmountNewError` collide as the
# same key. Path prefixes are stripped up to the last `::` followed by an
# uppercase letter (Rust type names are UpperCamelCase; crate/module path
# segments are snake_case). Path segments nested inside a generic argument are
# not special-cased (out of scope for this workspace).
#
# Exclusions (each with its justification):
#   - test code (files under tests/, items inside #[cfg(test)] modules, fns with
#     test attributes) — test errors are not part of the public contract and are
#     exercised per-test, so sharing there is not a caller-facing hazard.
#   - `fmt::Error` / `std::fmt::Error` / `core::fmt::Error` — forced by the
#     Display/Debug trait signatures.
#   - `Self::Error` — self-referential to the impl's own `type Error`, dictated
#     by the TryFrom/FromStr trait signatures, never a shared concrete type.
#   - a bare single uppercase letter (`E`, `D`, …) — a generic error type
#     parameter of the fn/impl.
#   - `X::Error` where `X` is a single uppercase letter (`D::Error`, …) — an
#     associated error path on a generic type parameter, dictated by the trait
#     (serde Deserialize, std hash, …).

def trim: sub("^[[:space:]]+"; "") | sub("[[:space:]]+$"; "");

# Split a string on the first top-level comma (depth 0, counting < > ( ) [ ] { }).
def split_top_comma:
  . as $s
  | ($s | split("")) as $cs
  | reduce range(0; ($cs | length)) as $i (
      {d: 0, cur: "", out: []};
      . as $st
      | $cs[$i] as $c
      | if ($c == "<" or $c == "(" or $c == "[" or $c == "{") then
          {d: ($st.d + 1), cur: ($st.cur + $c), out: $st.out}
        elif ($c == ">" or $c == ")" or $c == "]" or $c == "}") then
          {d: ($st.d - 1), cur: ($st.cur + $c), out: $st.out}
        elif ($c == "," and $st.d == 0) then
          {d: $st.d, cur: "", out: ($st.out + [$st.cur])}
        else
          {d: $st.d, cur: ($st.cur + $c), out: $st.out}
        end
    )
  | .out + [.cur];

# Error type text of a `Result<…>` return type (the last type argument).
def result_err:
  sub("^.*[Rr]esult[[:space:]]*<"; "") | sub(">[[:space:]]*$"; "") | split_top_comma | .[-1] | trim;

# Error type text of `type Error = E;`.
def assoc_err:
  sub("^[[:space:]]*type[[:space:]]+Error[[:space:]]*=[[:space:]]*"; "") | sub(";[[:space:]]*$"; "") | trim;

# Fixed error type text of `type Result<…> = …Result<…, E>;`.
def alias_err:
  sub("^[[:space:]]*type[[:space:]]+Result[[:space:]]*<[^=]*=[[:space:]]*"; "") | sub(";[[:space:]]*$"; "") | result_err;

# The `test-module` rule matches real `#[cfg(test)]` modules AND — because
# tree-sitter parses `Result<…>` return types with an error node that leaks into
# `kind: mod_item` — spuriously matches the enclosing function. Keep only real
# module declarations: text that starts with an optional visibility modifier
# followed by `mod `.
def is_real_mod:
  .text | test("^(pub([[:space:]]*\\([^)]*\\))?[[:space:]]+)?mod[[:space:]]");

# Normalize an error type to its grouping key.
def normalize: sub(".*::(?=[A-Z])"; "");

def is_fmt_err: . == "fmt::Error" or . == "std::fmt::Error" or . == "core::fmt::Error";
def is_self_err: test("^Self::");
def is_generic_param: test("^[A-Z]$");
def is_generic_assoc: test("^[A-Z]::Error$");
def is_boxed: test("^[Bb]ox<");
def is_anyhow: . == "anyhow::Error";

. as $all
| ($all | map(select(.ruleId == "test-module") | select(is_real_mod)) | map(.range.byteOffset)) as $testmods
| $all
| map(select(.ruleId == "error-slot-result" or .ruleId == "error-slot-assoc" or .ruleId == "result-alias"))
| map(
    . as $m
    | ($m.range.byteOffset) as $bo
    | ($m.file // "") as $file
    | ($m.range.start.line + 1) as $line
    | ($m.range.start.column + 1) as $col
    | (if $m.ruleId == "error-slot-result" then
         (($m.metaVariables.multi.secondary[0].text // "") | result_err)
       elif $m.ruleId == "error-slot-assoc" then
         (($m.text // "") | assoc_err)
       else
         (($m.text // "") | alias_err)
       end) as $err
    | (if $m.ruleId == "result-alias" then "alias" else "slot" end) as $role
    | {
        file: $file,
        line: $line,
        column: $col,
        role: $role,
        err: $err,
        in_test_file: ($file | test("(^|/)tests/")),
        in_testmod: ($testmods | any(.start <= $bo.start and $bo.start < .end))
      }
  )
# Turn each extracted slot into either a finding or a normalized group key.
| map(
    . as $x
    | if $x.role == "alias" then
        if $x.in_test_file or $x.in_testmod then empty
        else {file: $x.file, line: $x.line, column: $x.column, ruleId: "shared-error-types",
              message: ("`type Result<…> = …Result<…, \($x.err)>` alias fixes the error type and defeats the own-error-type rule; give each function a concrete error type instead")}
        end
      else
        if $x.in_test_file or $x.in_testmod then empty
        elif ($x.err | is_fmt_err) then empty
        elif ($x.err | is_self_err) then empty
        elif ($x.err | is_generic_param) then empty
        elif ($x.err | is_generic_assoc) then empty
        elif ($x.err | is_boxed) or ($x.err | is_anyhow) then
          {file: $x.file, line: $x.line, column: $x.column, ruleId: "shared-error-types",
           message: ("error slot uses `\($x.err)`, which is shared-by-construction; use a concrete per-function error type")}
        else
          {file: $x.file, line: $x.line, column: $x.column, key: ($x.err | normalize), raw: $x.err}
        end
      end
  )
| . as $findings
| ($findings | map(select(has("key"))) | group_by(.key)) as $groups
| [ $findings[]
    | if has("key") then
        (.key) as $k
        | ($groups | map(select(.[0].key == $k)) | .[0]) as $g
        | if ($g | length) > 1 then
            {file, line, column, ruleId: "shared-error-types",
             message: ("error type `\($k)` is returned by \($g | length) functions; give each function its own error type and wrap shared causes as variants")}
          else
            empty
          end
      else
        .
      end
  ]
| sort_by(.file, .line, .column)
| unique
