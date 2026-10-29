# record-processor-rs

Toy payments engine. Reads a CSV of transactions, applies them to per-client
accounts, writes the resulting account balances to stdout.

## Overview

`record-processor` streams a CSV of transactions and applies each one to a
per-client account, then writes one balance row per client to stdout.

```sh
cargo run -- transactions.csv > accounts.csv
```

Input columns:

| column | meaning |
| -------- | ----------------------- |
| `type` | transaction kind |
| `client` | `u16` client id |
| `tx` | `u32` transaction id |
| `amount` | decimal, up to 4 places |

Transaction kinds: `deposit`, `withdrawal`, `dispute`, `resolve`,
`chargeback`.

Output columns:

| column | meaning |
| ----------- | ------------------- |
| `client` | client id |
| `available` | `total - held` |
| `held` | funds under dispute |
| `total` | `available + held` |
| `locked` | account frozen |

Amounts are written to 4 decimal places.

## Approach

How the project is put together:

- **Workspace split by responsibility.** One crate per concern (see layout
  below); the root binary is a thin entry point.
- **Strict lints.** `clippy` with every group denied — including `restriction`
  — plus ast-grep rules that ban panics, indexing, fallbacks, and lint
  suppressions.
- **Test-first.** Behaviour is driven from a failing test before the
  implementation; tests run through the `rudzio` runner under a line-coverage
  gate.
- **Streaming by design (goal).** Records are processed one at a time rather
  than loading the whole file, so the engine scales to arbitrarily large
  inputs; transaction ids span the full `u32` range. *(Planned — engine not
  yet implemented.)*
- **Fixed-point money.** `rust_decimal` holds all amounts; floats are never
  used for money. *(Planned.)*
- **Errors surfaced loudly.** Typed errors via `thiserror` propagate to the
  caller; nothing is silently swallowed. *(Planned.)*
- **Malformed partner references ignored.** A `dispute`, `resolve`, or
  `chargeback` naming an unknown transaction, or a transaction not under
  dispute, is ignored as a partner-side error. *(Planned.)*

## Workspace layout

| crate | responsibility | dependencies |
| ------------------------ | --------------------------------- | --------------------------------------------- |
| `record-processor` (bin) | thin entry, calls `cli` | `cli` |
| `crates/domain` | ids, amounts, transactions, accounts | `rust_decimal` |
| `crates/storage` | account and transaction state | `domain`, `rust_decimal` |
| `crates/service` | engine applying transactions | `domain`, `storage`, `rust_decimal`, `thiserror` |
| `crates/transport` | CSV reading and writing | `csv`, `serde`, `domain`, `rust_decimal` |
| `crates/cli` | argument parsing and wiring | `pico-args`, `transport`, `service`, `storage`, `thiserror` |

Shared workspace dependencies, pinned to exact versions: `anyhow` 1.0.104
(dev), `compio` 0.19.2, `csv` 1.4.0, `pico-args` 0.5.0, `rust_decimal` 1.43.0,
`serde` 1.0.229, `thiserror` 2.0.21, and `rudzio` (git, the test runner).

## Development

- **Dev shell:** `nix develop` (Nix flake). The toolchain is pinned to 1.99.1
  via `rust-toolchain.toml`, with the `rustfmt`, `clippy`, `rust-src`, and
  `rust-analyzer` components.

- **Tasks:** a Justfile with five recipes.

  | recipe | action |
  | ------- | ------------------------------------------------ |
  | `build` | `cargo build` the whole workspace |
  | `test` | run tests via `rudzio` under a coverage gate |
  | `lint` | full lint pipeline (see below) |
  | `fmt` | apply all formatters |
  | `clean` | remove build artifacts |

- **Pre-commit hook:** runs `just lint` (wired via
  `core.hooksPath=scripts/hooks`).

- **Lint pipeline:** format check (`cargo fmt`, `taplo`, `mdformat`, `nixfmt`,
  `just --fmt`, `fish_indent`, `yamlfmt`), fish syntax check, `clippy` with all
  groups denied, ast-grep panic/unwrap/indexing bans, non-fish script check,
  yaml lint, recipe-name check, no-suppressions, no-banner-comments,
  workspace-deps, non-rudzio, and no-fallbacks.

- **Tests:** `rudzio` runner with a line-coverage gate (default 96%).

## Assumptions

Assumptions are documented here as the engine is implemented. The spec-given
ones:

- Each client has a single asset account.
- Clients are created on first reference.
- Client ids are `u16`; transaction ids are `u32`.
- Transactions occur chronologically in the file.

## Status

Scaffold only. The engine is not implemented; `just test` currently fails
because there are no tests yet, so the coverage gate has no data.

## License

MIT. See [LICENSE](LICENSE).
