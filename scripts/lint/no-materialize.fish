#!/usr/bin/env fish
# no-materialize linter for the record-processor Rust workspace.
#
# Forbids materializing domain types into heap collections: `Vec<D>`,
# `VecDeque<D>`, and `Box<[D]>` where `D` is one of the domain types — the
# `pub struct`/`pub enum` names declared in `crates/domain/src`, discovered at
# lint time. The pipeline streams domain records lazily through memory-mapped
# buffers instead of accumulating them on the heap, so any such collection is a
# code smell.
#
# Test code is exempt (structurally — no path allow-lists): files under
# `tests/` and items inside `#[cfg(test)]` modules are dropped by byte-range
# subtraction, exactly as shared-error-types excludes test code.
#
# Usage (run from the repo root):
#   fish scripts/lint/no-materialize.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for findings, one JSON object per line.
#   Defaults to `tmp/no-materialize.ndjson`. The `just lint` orchestrator
#   (scripts/lint.fish) passes `tmp/no-materialize.ndjson`.
#
# Exit status:
#   0 — self-tests passed and no domain type was materialized.
#   1 — self-tests failed, domain-type discovery failed, ast-grep/jq errored,
#       or a domain type was materialized.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
set -g config (path resolve "$script_dir/no-materialize/sgconfig.yml")
set -g filter "$repo_root/.jq/no-materialize.jq"

cd "$repo_root"

set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/no-materialize.ndjson"
end
mkdir -p (path dirname "$out")

if not test -f "$filter"
    echo "✗ no-materialize: filter file missing: $filter" >&2
    exit 1
end

# Discover the domain types: every `pub struct Name` / `pub enum Name` in
# crates/domain/src. A silent empty set would make the lint a no-op, so fail
# loudly if discovery yields nothing.
set -l domain_types (grep -rhoE '^pub (struct|enum) [A-Z][A-Za-z0-9_]*' crates/domain/src/ | string replace -r '^pub (struct|enum) ' '' | sort -u)
if test (count $domain_types) -eq 0
    echo "✗ no-materialize: no domain types found under crates/domain/src/ — aborting" >&2
    exit 1
end
set -g domains_json (jq -cn '$ARGS.positional' --args $domain_types)

function _run_pipeline --argument-names dir outfile
    set -l raw (mktemp)
    set -l stderr_file (mktemp)
    ast-grep scan --config "$config" --json=stream "$dir" >"$raw" 2>"$stderr_file"
    set -l scan_status $status
    if test $scan_status -gt 1
        echo "✗ no-materialize: ast-grep scan failed (exit $scan_status)" >&2
        cat "$stderr_file" >&2
        rm -f "$raw" "$stderr_file"
        return 1
    end
    rm -f "$stderr_file"
    jq -s --argjson domains "$domains_json" -f "$filter" "$raw" | jq -c '.[]' >"$outfile"
    or begin
        rm -f "$raw"
        return 1
    end
    rm -f "$raw"
    return 0
end

function _self_test
    set -g selftest_dir "tmp/no-materialize-selftest-$fish_pid"
    set -l dir "$selftest_dir"
    mkdir -p "$dir/flag" "$dir/pass/tests"

    function _selftest_cleanup --on-event fish_exit
        rm -rf "$selftest_dir"
    end

    # Flag cases: a heap collection of a discovered domain type in non-test code.
    # `Transaction` and `ClientId` are real `pub` domain types.
    printf '%s\n' \
        'fn f() {' \
        '    let xs: Vec<Transaction> = todo!();' \
        '    let ys: VecDeque<ClientId> = todo!();' \
        '    let zs: Box<[Transaction]> = todo!();' \
        '    let ws: Vec<Transaction, A> = Vec::new_in(alloc);' \
        '}' >"$dir/flag/mat.rs"

    set -l flag_out (mktemp)
    _run_pipeline "$dir/flag" "$flag_out"
    or return 1
    set -l flag_total (string trim (wc -l < "$flag_out"))
    if test "$flag_total" -lt 4
        echo "✗ no-materialize: self-test failed — expected >=4 findings, got $flag_total" >&2
        jq -c . "$flag_out" >&2
        return 1
    end
    rm -f "$flag_out"

    # Pass cases: non-domain heap collections, and domain collections in test code.
    printf '%s\n' \
        'fn f() {' \
        '    let xs: Vec<u8> = todo!();' \
        '    let ys: Vec<String> = todo!();' \
        '    let zs: Vec<T, A> = Vec::new_in(alloc);' \
        '}' >"$dir/pass/non-domain.rs"
    printf '%s\n' \
        '#[cfg(test)]' \
        'mod tests {' \
        '    #[test]' \
        '    fn t() {' \
        '        let xs: Vec<Transaction> = todo!();' \
        '    }' \
        '}' >"$dir/pass/testmod.rs"
    printf '%s\n' \
        '#[test]' \
        'fn it() {' \
        '    let xs: Vec<ClientId> = todo!();' \
        '}' >"$dir/pass/tests/it.rs"

    set -l pass_out (mktemp)
    _run_pipeline "$dir/pass" "$pass_out"
    or return 1
    set -l pass_total (string trim (wc -l < "$pass_out"))
    if test "$pass_total" -ne 0
        echo "✗ no-materialize: self-test failed — permitted fixture flagged" >&2
        jq -c . "$pass_out" >&2
        return 1
    end
    rm -f "$pass_out"

    echo "✓ no-materialize: self-tests passed"
    return 0
end

echo "ℹ no-materialize: running rule self-tests..."
if not ast-grep test --config "$config" --skip-snapshot-tests
    echo "✗ no-materialize: rule self-tests failed — aborting" >&2
    exit 1
end

echo "ℹ no-materialize: running self-tests..."
if not _self_test
    exit 1
end

echo "ℹ no-materialize: scanning workspace..."
set -l raw (mktemp)
set -l stderr_file (mktemp)
ast-grep scan --config "$config" --json=stream . >"$raw" 2>"$stderr_file"
set -l scan_status $status
if test $scan_status -gt 1
    echo "✗ no-materialize: ast-grep scan failed (exit $scan_status)" >&2
    cat "$stderr_file" >&2
    rm -f "$raw" "$stderr_file"
    exit 1
end
rm -f "$stderr_file"
jq -s --argjson domains "$domains_json" -f "$filter" "$raw" | jq -c '.[]' >"$out"
or begin
    echo "✗ no-materialize: jq classification failed" >&2
    rm -f "$raw"
    exit 1
end
rm -f "$raw"

set -l total (string trim (wc -l < "$out"))
if test "$total" -gt 0
    echo "✗ no-materialize: $total materialized domain type(s):"
    echo
    jq -r '"\(.file):\(.line):\(.column)  no-materialize  \(.message)"' "$out"
    echo
    echo "Per-file counts:"
    jq -r '.file' "$out" | sort | uniq -c | sort -rn
    echo
else
    echo "✓ no-materialize: no domain type materialized into a heap collection"
end
echo "ℹ no-materialize: JSON stream → $out"

if test "$total" -gt 0
    echo "✗ no-materialize: domain type materialized — failing lint" >&2
    exit 1
end
exit 0
