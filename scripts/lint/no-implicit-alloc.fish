#!/usr/bin/env fish
# no-implicit-alloc linter for the record-processor Rust workspace.
#
# Forbids implicit heap allocation in production code: `Vec::new` /
# `Vec::with_capacity` / `VecDeque::new` / `HashMap::new` / `BTreeMap::new` /
# `HashSet::new` / `BTreeSet::new` / `Box::new` / `Box::pin` / `String::new` /
# `String::from` / `String::with_capacity` / `Rc::new` / `Arc::new` /
# `vec![]` / `format!`, and the iterator/slice materializers `.collect` (in
# every form, including turbofish), `.to_vec`, and `.into_boxed_slice`.
#
# The record-processor pipeline must not heap-allocate: input events are logged
# to a memory-mapped append-only file and state to a memory-mapped KV store, so
# any of these constructors is a code smell to replace with a memory-mapped,
# reused-buffer, or borrowed alternative. (Choice documented: ALL `.collect` in
# non-test code is forbidden, not just collection-typed `collect`, because a
# bare `collect` into `Vec<_>`/`String`/… is the common implicit-allocation
# footgun and a targeted exemption would be easy to bypass.)
#
# Test code is exempt (structurally — no path allow-lists): files under
# `tests/` and items inside `#[cfg(test)]` modules are dropped by
# byte-range subtraction, exactly as shared-error-types excludes test code.
#
# Usage (run from the repo root):
#   fish scripts/lint/no-implicit-alloc.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for findings, one JSON object per line.
#   Defaults to `tmp/no-implicit-alloc.ndjson`. The `just lint` orchestrator
#   (scripts/lint.fish) passes `tmp/no-implicit-alloc.ndjson`.
#
# Exit status:
#   0 — self-tests passed and no implicit heap allocation was found.
#   1 — self-tests failed, ast-grep/jq errored, or an allocation was found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
set -g config (path resolve "$script_dir/no-implicit-alloc/sgconfig.yml")
set -g filter "$repo_root/.jq/no-implicit-alloc.jq"

cd "$repo_root"

set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/no-implicit-alloc.ndjson"
end
mkdir -p (path dirname "$out")

if not test -f "$filter"
    echo "✗ no-implicit-alloc: filter file missing: $filter" >&2
    exit 1
end

function _run_pipeline --argument-names dir outfile
    set -l raw (mktemp)
    set -l stderr_file (mktemp)
    ast-grep scan --config "$config" --json=stream "$dir" >"$raw" 2>"$stderr_file"
    set -l scan_status $status
    if test $scan_status -gt 1
        echo "✗ no-implicit-alloc: ast-grep scan failed (exit $scan_status)" >&2
        cat "$stderr_file" >&2
        rm -f "$raw" "$stderr_file"
        return 1
    end
    rm -f "$stderr_file"
    jq -s -f "$filter" "$raw" | jq -c '.[]' >"$outfile"
    or begin
        rm -f "$raw"
        return 1
    end
    rm -f "$raw"
    return 0
end

function _self_test
    set -g selftest_dir "tmp/no-implicit-alloc-selftest-$fish_pid"
    set -l dir "$selftest_dir"
    mkdir -p "$dir/flag" "$dir/pass/tests"

    function _selftest_cleanup --on-event fish_exit
        rm -rf "$selftest_dir"
    end

    # Flag cases: allocations in production (non-test) code.
    printf '%s\n' \
        'fn f() {' \
        '    let a = Vec::new();' \
        '    let b = vec![1, 2, 3];' \
        '    let c = format!("{x}");' \
        '    let d = it.collect::<Vec<_>>();' \
        '}' >"$dir/flag/alloc.rs"

    set -l flag_out (mktemp)
    _run_pipeline "$dir/flag" "$flag_out"
    or return 1
    set -l flag_total (string trim (wc -l < "$flag_out"))
    if test "$flag_total" -lt 4
        echo "✗ no-implicit-alloc: self-test failed — expected >=4 findings, got $flag_total" >&2
        jq -c . "$flag_out" >&2
        return 1
    end
    rm -f "$flag_out"

    # Pass cases: allocations only inside test code.
    printf '%s\n' \
        '#[cfg(test)]' \
        'mod tests {' \
        '    #[test]' \
        '    fn t() {' \
        '        let v = Vec::new();' \
        '    }' \
        '}' >"$dir/pass/testmod.rs"
    printf '%s\n' \
        '#[test]' \
        'fn it() {' \
        '    let v = vec![1, 2];' \
        '}' >"$dir/pass/tests/it.rs"
    printf '%s\n' \
        'fn clean(x: usize) -> usize { x }' >"$dir/pass/clean.rs"
    printf '%s\n' \
        'fn explicit<A>(alloc: A, hasher: S) {' \
        '    let a = Vec::new_in(alloc);' \
        '    let b = Vec::with_capacity_in(4, alloc);' \
        '    let c = HashMap::with_hasher_in(hasher, alloc);' \
        '}' >"$dir/pass/explicit.rs"

    set -l pass_out (mktemp)
    _run_pipeline "$dir/pass" "$pass_out"
    or return 1
    set -l pass_total (string trim (wc -l < "$pass_out"))
    if test "$pass_total" -ne 0
        echo "✗ no-implicit-alloc: self-test failed — test-code fixture flagged" >&2
        jq -c . "$pass_out" >&2
        return 1
    end
    rm -f "$pass_out"

    echo "✓ no-implicit-alloc: self-tests passed"
    return 0
end

echo "ℹ no-implicit-alloc: running rule self-tests..."
if not ast-grep test --config "$config" --skip-snapshot-tests
    echo "✗ no-implicit-alloc: rule self-tests failed — aborting" >&2
    exit 1
end

echo "ℹ no-implicit-alloc: running self-tests..."
if not _self_test
    exit 1
end

echo "ℹ no-implicit-alloc: scanning workspace..."
set -l raw (mktemp)
set -l stderr_file (mktemp)
ast-grep scan --config "$config" --json=stream . >"$raw" 2>"$stderr_file"
set -l scan_status $status
if test $scan_status -gt 1
    echo "✗ no-implicit-alloc: ast-grep scan failed (exit $scan_status)" >&2
    cat "$stderr_file" >&2
    rm -f "$raw" "$stderr_file"
    exit 1
end
rm -f "$stderr_file"
jq -s -f "$filter" "$raw" | jq -c '.[]' >"$out"
or begin
    echo "✗ no-implicit-alloc: jq classification failed" >&2
    rm -f "$raw"
    exit 1
end
rm -f "$raw"

set -l total (string trim (wc -l < "$out"))
if test "$total" -gt 0
    echo "✗ no-implicit-alloc: $total implicit heap allocation(s):"
    echo
    jq -r '"\(.file):\(.line):\(.column)  no-implicit-alloc  \(.message)"' "$out"
    echo
    echo "Per-file counts:"
    jq -r '.file' "$out" | sort | uniq -c | sort -rn
    echo
else
    echo "✓ no-implicit-alloc: no implicit heap allocations"
end
echo "ℹ no-implicit-alloc: JSON stream → $out"

if test "$total" -gt 0
    echo "✗ no-implicit-alloc: implicit heap allocations found — failing lint" >&2
    exit 1
end
exit 0
