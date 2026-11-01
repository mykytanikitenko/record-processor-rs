#!/usr/bin/env fish
# shared-error-types linter for the record-processor Rust workspace.
#
# Every fallible fn/method must own its OWN error type. One error type may not
# be the error of two different functions. Wrapping a child error as a variant
# (`#[from]`/`#[source]`) is fine; reusing the same return error type is not.
#
# Definition of an "error slot" (see .jq/shared-error-types.jq for the full
# spec):
#   (a) the second type argument `E` of `Result<T, E>` in the return type of a
#       fn/method (free fn, inherent method, trait method decl, trait impl
#       method);
#   (b) `type Error = E;` inside an impl block (TryFrom, FromStr, …).
#
# Normalization: `E` is reduced to its last path segment + generics as written,
# so `crate::amount::AmountNewError` and `AmountNewError` collide as one key.
#
# Cross-file counting cannot be done by a single ast-grep rule, so extraction is
# done structurally by ast-grep (scripts/lint/shared-error-types/rules/), then
# the raw stream is normalized, grouped and counted by jq
# (.jq/shared-error-types.jq). Findings point at EVERY site of a shared key.
#
# Exclusions: test code (tests/ dirs, #[cfg(test)] modules, test-attribute
# fns); fmt::Error / std::fmt::Error / core::fmt::Error (Display/Debug);
# generic error params and associated paths on them (D::Error, bare E);
# Self::Error (self-referential). Result aliases with a fixed error and
# Box<dyn Error>/anyhow::Error slots are separate, unconditional findings.
#
# Usage (run from the repo root):
#   fish scripts/lint/shared-error-types.fish [OUTPUT_NDJSON]
#
#   OUTPUT_NDJSON — destination for findings, one JSON object per line.
#   Defaults to `tmp/shared-error-types.ndjson`. The `just lint` orchestrator
#   (scripts/lint.fish) passes `tmp/shared-error-types.ndjson`.
#
# Exit status:
#   0 — self-tests passed and no shared error type was found.
#   1 — self-tests failed, ast-grep/jq errored, or shared error types were
#       found.

set -l script_dir (status dirname)
set -l repo_root (path resolve "$script_dir/../..")
set -g config (path resolve "$script_dir/shared-error-types/sgconfig.yml")
set -g filter "$repo_root/.jq/shared-error-types.jq"

cd "$repo_root"

set -l out "$argv[1]"
if test -z "$out"
    set out "tmp/shared-error-types.ndjson"
end
mkdir -p (path dirname "$out")

# The jq classifier is a hard dependency: if absent, the sub-lint would produce
# no findings and falsely report PASS. Fail loudly instead.
if not test -f "$filter"
    echo "✗ shared-error-types: filter file missing: $filter" >&2
    exit 1
end

# Run the extraction + classification pipeline over one directory, writing the
# finding array to $outfile. Shared by the real scan and the runtime self-test.
function _run_pipeline --argument-names dir outfile
    set -l raw (mktemp)
    set -l stderr_file (mktemp)
    ast-grep scan --config "$config" --json=stream "$dir" >"$raw" 2>"$stderr_file"
    set -l scan_status $status
    if test $scan_status -gt 1
        echo "✗ shared-error-types: ast-grep scan failed (exit $scan_status)" >&2
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

# Runtime self-test: generate fixtures covering every should-flag / should-pass
# case and assert the classifier's outcome on each.
function _self_test
    set -g selftest_dir "tmp/shared-error-types-selftest-$fish_pid"
    set -l dir "$selftest_dir"
    mkdir -p "$dir/flag" "$dir/pass/tests"

    function _selftest_cleanup --on-event fish_exit
        rm -rf "$selftest_dir"
    end

    # Flag cases (should produce findings).
    # Two functions sharing one error type.
    printf '%s\n' \
        'fn first() -> Result<(), SharedErr> { Ok(()) }' \
        'fn second() -> Result<(), SharedErr> { Ok(()) }' >"$dir/flag/shared.rs"
    # Same error type reached via a longer path — must still collide.
    printf '%s\n' \
        'fn third() -> Result<(), crate::nested::SharedErr> { Ok(()) }' >"$dir/flag/path.rs"
    # An associated `type Error = SharedErr` (TryFrom) shares the same key.
    printf '%s\n' \
        'struct Thing;' \
        'impl TryFrom<u8> for Thing {' \
        '    type Error = SharedErr;' \
        '    fn try_from(_v: u8) -> Result<Self, Self::Error> { Ok(Thing) }' \
        '}' >"$dir/flag/assoc.rs"
    # A Result alias fixing the error type defeats the rule.
    printf '%s\n' \
        'type Result<T> = std::result::Result<T, AliasErr>;' >"$dir/flag/alias.rs"
    # A boxed error is shared-by-construction.
    printf '%s\n' \
        'fn boxed() -> Result<(), Box<dyn std::error::Error>> { Ok(()) }' >"$dir/flag/boxed.rs"

    set -l flag_out (mktemp)
    _run_pipeline "$dir/flag" "$flag_out"
    or return 1
    set -l shared_count (jq -c 'select(.message | contains("SharedErr"))' "$flag_out" | wc -l | string trim)
    set -l alias_count (jq -c 'select(.message | contains("AliasErr"))' "$flag_out" | wc -l | string trim)
    set -l boxed_count (jq -c 'select(.message | contains("Box<dyn"))' "$flag_out" | wc -l | string trim)
    # SharedErr appears in 4 slots (2 in shared.rs, 1 in path.rs, 1 assoc type).
    if test "$shared_count" -ne 4
        echo "✗ shared-error-types: self-test failed — shared error not flagged (expected 4 sites, got $shared_count)" >&2
        jq -c . "$flag_out" >&2
        return 1
    end
    if test "$alias_count" -ne 1
        echo "✗ shared-error-types: self-test failed — Result alias not flagged" >&2
        return 1
    end
    if test "$boxed_count" -ne 1
        echo "✗ shared-error-types: self-test failed — boxed error not flagged" >&2
        return 1
    end
    rm -f "$flag_out"

    # Pass cases (should produce no findings).
    # Unique error type per function.
    printf '%s\n' \
        'fn a() -> Result<(), AErr> { Ok(()) }' \
        'fn b() -> Result<(), BErr> { Ok(()) }' >"$dir/pass/unique.rs"
    # Wrapping a child error as a variant: the fn returns the parent type only.
    printf '%s\n' \
        '#[derive(Debug)] enum ChildErr {}' \
        '#[derive(Debug)] enum ParentErr { Child(ChildErr) }' \
        'fn parent() -> Result<(), ParentErr> { Ok(()) }' \
        'fn child() -> Result<(), ChildErr> { Ok(()) }' >"$dir/pass/wrapped.rs"
    # A self-referential associated error (Self::AppendError) used by the trait
    # declaration and its impl is not a shared type: it names the trait's own
    # associated type, exactly like Self::Error.
    printf '%s\n' \
        'trait Append {' \
        '    type AppendError: core::error::Error;' \
        '    async fn append(&self) -> Result<(), Self::AppendError>;' \
        '}' \
        'struct Impl;' \
        'impl Append for Impl {' \
        '    type AppendError = Concrete;' \
        '    async fn append(&self) -> Result<(), Self::AppendError> { Ok(()) }' \
        '}' \
        '#[derive(Debug)] enum Concrete {}' >"$dir/pass/self_ref.rs"
    # fmt::Error in a Display impl (forced by the trait).
    printf '%s\n' \
        'use std::fmt;' \
        'struct Thing;' \
        'impl fmt::Display for Thing {' \
        '    fn fmt(&self, _f: &mut fmt::Formatter<'"'"'_>) -> std::result::Result<(), fmt::Error> { Ok(()) }' \
        '}' >"$dir/pass/display.rs"
    # D::Error in a Deserialize impl (generic associated path, trait-dictated).
    printf '%s\n' \
        'struct Thing;' \
        'impl<'"'"'de> serde::Deserialize<'"'"'de> for Thing {' \
        '    fn deserialize<D>(_d: D) -> Result<Self, D::Error>' \
        '    where D: serde::Deserializer<'"'"'de> { todo!() }' \
        '}' >"$dir/pass/deserialize.rs"
    # #[cfg(test)] module sharing an error type with a tests/ dir — both test code.
    printf '%s\n' \
        '#[cfg(test)]' \
        'mod tests {' \
        '    fn t() -> Result<(), TestSharedErr> { Ok(()) }' \
        '}' >"$dir/pass/testcode.rs"
    printf '%s\n' \
        'fn t2() -> Result<(), TestSharedErr> { Ok(()) }' >"$dir/pass/tests/it.rs"

    set -l pass_out (mktemp)
    _run_pipeline "$dir/pass" "$pass_out"
    or return 1
    set -l pass_total (string trim (wc -l < "$pass_out"))
    if test "$pass_total" -ne 0
        echo "✗ shared-error-types: self-test failed — clean fixture flagged" >&2
        jq -c . "$pass_out" >&2
        return 1
    end
    rm -f "$pass_out"

    echo "✓ shared-error-types: self-tests passed"
    return 0
end

# 1. Self-test the extraction rules before scanning.
echo "ℹ shared-error-types: running rule self-tests..."
if not ast-grep test --config "$config" --skip-snapshot-tests
    echo "✗ shared-error-types: rule self-tests failed — aborting" >&2
    exit 1
end

# 2. Runtime self-test of the full extraction + counting pipeline.
echo "ℹ shared-error-types: running self-tests..."
if not _self_test
    exit 1
end

# 3. Scan the whole workspace and classify.
echo "ℹ shared-error-types: scanning workspace..."
set -l raw (mktemp)
set -l stderr_file (mktemp)
ast-grep scan --config "$config" --json=stream . >"$raw" 2>"$stderr_file"
set -l scan_status $status
if test $scan_status -gt 1
    echo "✗ shared-error-types: ast-grep scan failed (exit $scan_status)" >&2
    cat "$stderr_file" >&2
    rm -f "$raw" "$stderr_file"
    exit 1
end
rm -f "$stderr_file"
jq -s -f "$filter" "$raw" | jq -c '.[]' >"$out"
or begin
    echo "✗ shared-error-types: jq classification failed" >&2
    rm -f "$raw"
    exit 1
end
rm -f "$raw"

# 4. Human-readable diagnostics + summary.
set -l total (string trim (wc -l < "$out"))
if test "$total" -gt 0
    echo "✗ shared-error-types: $total shared error type finding(s):"
    echo
    jq -r '"\(.file):\(.line):\(.column)  shared-error-types  \(.message)"' "$out"
    echo
    echo "Per-file counts:"
    jq -r '.file' "$out" | sort | uniq -c | sort -rn
    echo
else
    echo "✓ shared-error-types: every fallible function owns its error type"
end
echo "ℹ shared-error-types: JSON stream → $out"

if test "$total" -gt 0
    echo "✗ shared-error-types: shared error types found — failing lint" >&2
    exit 1
end
exit 0
