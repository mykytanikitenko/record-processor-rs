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
| `crates/storage` | persistence only: event queue, dedup index, end-of-input signal, per-client state | `domain`, `thiserror`, `tokio-util` |
| `crates/service` | all business rules, generic over the storage traits | `domain`, `storage`, `thiserror`, `tokio-util` |
| `crates/transport` | stdio CSV reading and writing | `compio`, `domain`, `rust_decimal`, `service`, `thiserror`, `tokio-util` |
| `crates/cli` | composition root: builds real storage, service and transport and wires them | `compio`, `pico-args`, `service`, `storage`, `transport`, `thiserror`, `tokio-util` |

Shared workspace dependencies, pinned to exact versions: `anyhow` 1.0.104
(dev), `compio` 0.19.2, `csv` 1.4.0, `mockall` 0.15.0 (dev, `service` only),
`pico-args` 0.5.0, `rust_decimal` 1.43.0, `serde` 1.0.229, `thiserror` 2.0.21,
`tokio-util` 0.7.20, and `rudzio` (git, the test runner).

### Layering and trait contracts

Every async contract method takes a `&CancellationToken` and returns its own
error type, so a failure names the operation that produced it.

**Storage** (`crates/storage`) stores what it is given and decides nothing.
Business outcomes such as a duplicate id are reported as values, and the
service decides what they mean. It is tested by integration tests in
`crates/storage/tests/` against the real backend; it has no mocks.

| trait | methods |
| --- | --- |
| `EventQueue` | `append_batch(&[Transaction]) -> IdRange`; `read_batch(from: u64, &mut [Transaction]) -> usize` fills a caller-owned buffer |
| `DedupIndex` | `insert(TxId) -> Insertion` (`Fresh` or `Duplicate`) |
| `EndOfInput` | `notify(end: u64)`; `wait() -> u64` returns the queue's exclusive end id |
| `KvState` | `read(ClientId) -> Option<AccountBalance>`; `write(ClientId, &AccountBalance)`; `entries() -> KvEntries`, a lazy cursor |

`storage::Storage` is the production backend. Every method returns a typed
`Unimplemented` until the mmap-backed store lands.

**Service** (`crates/service`) owns every rule: deposit, withdrawal, dispute,
resolve, chargeback, routing and failed-transaction bookkeeping. `Engine<Q, I, E, K>` holds one handle per storage trait. That lets each handle be the real
store or a mock independently. `Service::serve(self, TransactionStream, &CancellationToken) -> Output` returns a lazy `BalanceStream`. Unit tests in
`crates/service/tests/` drive the engine against `mockall` mocks of all four
storage traits.

**Transport** (`crates/transport`) implements `TransactionStream` (the CSV
`Reader`) and writes balances (`Writer`) over stdio or a file. There is one real
implementation, and the traits leave room for other protocols.

**CLI** (`crates/cli`) is the composition root. It constructs
`Engine { eof, index, queue, state }` over `storage::Storage` and feeds it the
transport reader.

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

The step-1 traits contract is landed: `service`, `transport`, and `storage`
traits with `Real`/`Mock` dispatch, cancellation plumbing, and CLI wiring are
in place; `just build` and `just lint` are green. The transport layer is being
rewritten (Compio `Framed` + `csv-core`). The engine (business logic) is not
implemented — `serve` is a contract stub returning `EngineError::Unimplemented`
— so the e2e success fixtures fail until the engine lands.

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

## Open decisions for review

Collected here for operator review: each entry records the id, the open
question, the current behaviour, and where the decision lives. The eleven
fixture cases are `pending-decision` in the e2e corpus — the harness fails
loudly with `pending decision <id>: <question>` (`tests/e2e/cases.rs:76`)
without running the binary, so their current behaviour is "not decided"; no
engine code implements them yet.

- **`failed-withdrawal-new-client`** — When a withdrawal targets a client with
  no prior record (and so has zero available funds), should that client appear
  in the output as a zero-balance row, or be omitted entirely?
  - Current behaviour: not decided; e2e case `033` fails loudly.
  - Decided in: not implemented yet.
- **`dispute-withdrawal`** — Can a dispute reference a withdrawal transaction?
  If so, does disputing a withdrawal reverse the debit (returning funds to
  available), or is dispute only meaningful for deposits?
  - Current behaviour: not decided; e2e case `080` fails loudly.
  - Decided in: not implemented yet.
- **`dispute-insufficient-available`** — When a dispute references a deposit
  whose amount exceeds the client's current available funds (part already
  withdrawn), should the dispute fail, drive available negative, or be clamped?
  - Current behaviour: not decided; e2e case `081` fails loudly.
  - Decided in: not implemented yet.
- **`locked-account-operations`** — After a chargeback locks an account, should
  subsequent deposits, withdrawals, disputes, and resolves on that account be
  rejected, or still applied?
  - Current behaviour: not decided; e2e case `082` fails loudly.
  - Decided in: not implemented yet.
- **`mismatched-client-reference`** — When a dispute/resolve/chargeback row
  names a client that differs from the client of the referenced transaction,
  which is authoritative — the row's client or the transaction's client?
  - Current behaviour: not decided; e2e case `083` fails loudly.
  - Decided in: not implemented yet.
- **`duplicate-tx-id`** — When a transaction id is reused by a later row, should
  the later row be ignored, treated as an error, or applied?
  - Current behaviour: not decided; e2e case `084` fails loudly.
  - Decided in: not implemented yet (see the dedup-index note below).
- **`redispute-after-resolve`** — After a dispute has been resolved, can the
  same transaction be disputed again?
  - Current behaviour: not decided; e2e case `085` fails loudly.
  - Decided in: not implemented yet.
- **`dispute-after-chargeback`** — After a chargeback finalizes a dispute and
  locks the account, can the same transaction be disputed again?
  - Current behaviour: not decided; e2e case `086` fails loudly.
  - Decided in: not implemented yet.
- **`double-dispute`** — Should disputing the same transaction twice hold the
  funds twice, ignore the second dispute, or be treated as an error?
  - Current behaviour: not decided; e2e case `087` fails loudly.
  - Decided in: not implemented yet.
- **`dispute-failed-withdrawal`** — Can a dispute reference a withdrawal that
  failed for insufficient funds? If so, what is disputed — nothing (the
  withdrawal never changed balances) or the attempted amount?
  - Current behaviour: not decided; e2e case `088` fails loudly.
  - Decided in: not implemented yet.
- **`ignored-row-client-output`** — When a client appears only in rows that are
  ignored (a dispute/resolve/chargeback of an unknown transaction), should that
  client appear as a zero-balance row in the output, or be omitted?
  - Current behaviour: not decided; e2e case `089` fails loudly.
  - Decided in: not implemented yet.

### Implementation questions

- **`failed-transaction-bookkeeping`** — a failure in a single event must not
  stop the pipeline; per-client state records failed transactions (tx id, kind,
  reason) — format and output exposure undecided.
  - Current behaviour: not implemented — no per-client failed-transaction state
    exists.
  - Decided in: not implemented yet.
- **`duplicate-tx-id-dedup-index`** — duplicate tx id handling in the storage
  dedup index (error) vs the fixture 084 question above.
  - Current behaviour: the storage dedup index `Index::insert` returns
    `IndexError::Duplicate(TxId)` for a reused transaction id.
  - Decided in: `crates/storage/src/dedup.rs:11`.

## License

MIT. See [LICENSE](LICENSE).
