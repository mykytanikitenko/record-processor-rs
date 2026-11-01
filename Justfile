# record-processor — Task Runner
# The core recipes (build, test, lint, fmt, clean) plus one per Rust sub-lint
# (lint-no-manual-fmt, lint-no-std-io, lint-no-std-fs, lint-no-implicit-alloc,
# lint-no-materialize, lint-no-tokio-runtime). Each is a thin wrapper over
# scripts/<name>.fish, which auto-enters the nix dev shell when run outside of
# one.

set shell := ["fish", "-c"]

# Build the entire workspace.
build:
    @fish scripts/build.fish

# Run every workspace test through the shared cargo-rudzio runner.
# `just test --html` additionally writes an llvm-cov HTML report to
# tmp/coverage/; `just test --threshold N` sets the coverage gate. Other
# arguments pass through to `cargo rudzio test`.
test *args:
    @fish scripts/test.fish {{ args }}

# Run the full lint pipeline: format check, fish syntax, clippy, panic
# patterns, non-fish scripts, yaml lint, recipe names, no-suppressions,
# no-banner-comments, workspace-deps, non-rudzio, no-fallbacks, cargo-audit,
# cargo-deny, and cargo-machete.
lint *args:
    @fish scripts/lint.fish {{ args }}

# Run the no-manual-fmt sub-lint in isolation.
lint-no-manual-fmt:
    @fish scripts/lint/no-manual-fmt.fish

# Run the no-std-io sub-lint in isolation.
lint-no-std-io:
    @fish scripts/lint/no-std-io.fish

# Run the no-std-fs sub-lint in isolation.
lint-no-std-fs:
    @fish scripts/lint/no-std-fs.fish

# Run the no-implicit-alloc sub-lint in isolation.
lint-no-implicit-alloc:
    @fish scripts/lint/no-implicit-alloc.fish

# Run the no-materialize sub-lint in isolation.
lint-no-materialize:
    @fish scripts/lint/no-materialize.fish

# Run the no-tokio-runtime sub-lint in isolation.
lint-no-tokio-runtime:
    @fish scripts/lint/no-tokio-runtime.fish

# Run the no-global-allocator sub-lint in isolation.
lint-no-global-allocator:
    @fish scripts/lint/no-global-allocator.fish

# Run the no-to-string sub-lint in isolation.
lint-no-to-string:
    @fish scripts/lint/no-to-string.fish

# Apply all formatters (Rust, TOML, Markdown, Nix, Justfile).
fmt:
    @fish scripts/fmt.fish

# Remove build artifacts. `just clean` clears target/ and tmp/;
# `just clean --stale` clears only stale ephemeral artifacts.
clean *args:
    @fish scripts/clean.fish {{ args }}
