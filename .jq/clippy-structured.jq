# Collapse a slurped array of cargo json messages into structured diagnostics.
# Input: `jq -s` of a clippy `--message-format=json` ndjson stream.
# Output: an array of `{level, code, message, file, line_start, column_start}`
# objects — one per cargo diagnostic, taken from its primary span, deduped.
[
  .[]
  | select(.reason == "compiler-message")
  | .message as $m
  | ($m.spans // []) as $spans
  | (($spans | map(select(.is_primary == true)) | .[0]) // ($spans[0] // null)) as $s
  | select($s != null)
  | {
      level: $m.level,
      code: ($m.code.code // null),
      message: $m.message,
      file: $s.file_name,
      line_start: $s.line_start,
      column_start: $s.column_start
    }
] | unique
