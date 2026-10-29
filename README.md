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

Amounts are written in their shortest decimal form (no fixed four-place
padding): `1.5`, not `1.5000`.

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
  used for money.
- **Errors surfaced loudly.** Typed errors via `thiserror` propagate to the
  caller; nothing is silently swallowed.
- **Input and output models separated.** `input` holds transactions read from
  CSV (with a strictly positive `Amount`); `output` holds account-balance rows
  whose `total = available + held` invariant is enforced by the constructor.
- **Malformed partner references ignored.** A `dispute`, `resolve`, or
  `chargeback` naming an unknown transaction, or a transaction not under
  dispute, is ignored as a partner-side error. *(Planned.)*

## Workspace layout

| crate | responsibility | dependencies |
| ------------------------ | --------------------------------- | --------------------------------------------- |
| `record-processor` (bin) | thin entry, calls `cli` | `cli` |
| `crates/domain` | transaction and account-balance models | `rust_decimal`, `serde`, `thiserror` |
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
- Amounts are strictly positive with at most four decimal places; trailing
  zeros normalize, so `1.50000` is accepted as `1.5`.
- `Eof` is a stream-end signal emitted by the transport, not a CSV row; a
  literal `eof` row is rejected during deserialization.
- Output balances derive `total` from `available + held` in the constructor, so
  the invariant cannot be broken. Output decimal formatting uses the shortest
  representation (no fixed four-place padding), as recorded under
  [Decisions](#decisions).

## Decisions

Recorded decisions, each with the reasoning that settled it:

- **Malformed input rows abort the run.** A row that cannot be converted to a
  transaction — an unknown `type`, a missing or unexpected amount, a
  non-numeric or out-of-range client or transaction id, or an invalid amount —
  fails the whole run with a non-zero exit, an error on stderr, and nothing on
  stdout. This is bank-like fail-loud behaviour: corrupt partner data must not
  be silently skipped, because skipping a row would silently change the
  reported balances.
- **Output decimals use the shortest representation.** Balances are written
  without fixed four-place padding (`1.5`, not `1.5000`). The spec does not
  require a particular decimal display for round values, and the shortest form
  is unambiguous.

## Status

The domain model (`input` transactions and `output` balances) is implemented
and tested; `just build`, `just lint`, and `just test` are green. The engine
(`storage`, `service`, `transport`) is not yet implemented.

## End-to-end tests

The root package carries a `rudzio` integration-test target (`tests/e2e`,
declared as `[[test]] name = "e2e"` with `harness = false`). Each test drives
the compiled binary over a fixture's `input.csv` and checks stdout against the
case's expected output.

### Fixture corpus

`tests/e2e/fixtures/<NNN-name>/` holds one case per directory:

- `input.csv` — the transaction stream fed to the binary as its sole argument.
- `expected.csv` — the expected account output, present for success cases.
- `case.toml` — metadata: `description`, `derivation` (step-by-step
  arithmetic), `outcome` (`success`, `error`, or `pending-decision`), plus
  `decision` and `question` for pending-decision cases.

The corpus is systematic, covering transaction type × account/tx state × edge
condition: the spec example verbatim, deposit/withdrawal/dispute/resolve/
chargeback flows, boundary ids (`u16`/`u32` maximums and zero), whitespace and
decimal precision, large values, header-only input, malformed rows, and every
case the spec leaves open.

### Outcome handling

- **success** — the binary exits zero with empty stderr; stdout is parsed into
  typed rows (`u16`, `Decimal`, `Decimal`, `Decimal`, `bool`) and compared for
  whole-vector equality. Decimal equality is value-based, so `1.5` equals
  `1.5000`.
- **error** — the binary exits non-zero with empty stdout; exercised by the
  malformed-row cases (`061`–`071`).
- **pending-decision** — the test fails loudly with
  `pending decision <id>: <question>`, never skipped and never passing, until
  the policy is chosen.

### Open decisions

Cases the spec does not decide are recorded as pending decisions rather than
guessed at. See each `case.toml` for the precise question; the ids are
`failed-withdrawal-new-client`, `dispute-withdrawal`,
`dispute-insufficient-available`, `locked-account-operations`,
`mismatched-client-reference`, `duplicate-tx-id`, `redispute-after-resolve`,
`dispute-after-chargeback`, `double-dispute`, `dispute-failed-withdrawal`,
and `ignored-row-client-output`.

## License

MIT. See [LICENSE](LICENSE).
