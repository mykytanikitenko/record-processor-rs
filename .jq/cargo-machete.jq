# Normalize `cargo machete` human output into one compact JSON object per
# unused dependency.
#
# Expected input (one line per record, tab-indented dependency names under a
# `crate -- ./path/Cargo.toml:` header):
#   cli -- ./crates/cli/Cargo.toml:
#   	service
#   	storage
#
# Output: one JSON object per line:
#   {"file":"./crates/cli/Cargo.toml","line":0,"ruleId":"cargo-machete","message":"unused dependency service in cli"}
split("\n")
| reduce .[] as $line (
    {crate: "", path: "", findings: []};
    if ($line | test("^[^ \t]+ -- [^ \t]+Cargo\\.toml:$"))
    then
      .crate = ($line | sub(" -- .*$"; ""))
      | .path = ($line | sub("^[^ ]+ -- "; "") | sub(":$"; ""))
    elif ($line | test("^\t[^\t]"))
    then
      .findings += [
        {
          file: .path,
          line: 0,
          ruleId: "cargo-machete",
          message: ("unused dependency " + ($line | sub("^\t"; "")) + " in " + .crate)
        }
      ]
    else .
    end
  )
| .findings[]
