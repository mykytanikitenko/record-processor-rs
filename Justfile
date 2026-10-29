# record-processor — Task Runner
# Exactly five recipes: build, test, lint, fmt, clean (first recipe is the
# default, so bare `just` builds). Each is a thin wrapper over
# scripts/<name>.fish, which auto-enters the nix dev shell when run outside
# of one.

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

# Apply all formatters (Rust, TOML, Markdown, Nix, Justfile).
fmt:
    @fish scripts/fmt.fish

# Remove build artifacts. `just clean` clears target/ and tmp/;
# `just clean --stale` clears only stale ephemeral artifacts.
clean *args:
    @fish scripts/clean.fish {{ args }}
