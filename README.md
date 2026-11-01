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

## For reviewers

A walk through the architecture and the choices that shape it, so a review can
find the interesting parts fast. The codebase is small and strict: every
boundary is a trait, every fallible call has its own typed error, and no domain
value is ever cloned into an owned `String` or collected into a `Vec`.

### What it does

One streaming pipeline: read a CSV of transactions, apply each to a per-client
account, write one balance row per client to stdout. Records move through the
system as borrowed slices, and account state is persisted to memory-mapped
files as it is computed — no batch of the whole file ever lives on the heap.

### Layering and dependency direction

Dependencies point one way, and every layer is generic over the layer below it:

```text
domain ← storage ← service ← transport ← cli
```

- **domain** (`crates/domain`) — the value types: `Transaction`, `TxId`,
  `ClientId`, `Amount`, `AccountBalance`. It depends on nothing in the
  workspace. Money is `rust_decimal::Decimal`; floats never touch an amount.
- **storage** (`crates/storage`) — persistence only. The `Queue<T>` and
  `Kv<K, V>` contracts and their in-memory and file-backed implementations.
  It stores opaque `Copy` records and decides nothing about payments.
- **service** (`crates/service`) — every business rule. `Consumer` drains a
  transaction stream into engine state and projects balances. It is generic
  over `Q: Queue<Transaction>` and `S: State`, so it does not know whether the
  queue is a `Vec` or an mmap.
- **transport** (`crates/transport`) — stdio I/O only: a hand-written CSV
  reader and a balance writer, adapted to the slice streams the service
  consumes.
- **cli** (`crates/cli`) — the composition root. It builds the file-backed
  stores, wires them into the engine, and drives `transport::serve`.

Each boundary exists to keep one concern from leaking across it: storage stays
free of business logic (the engine's *input* state is service-owned for exactly
this reason — see the module doc in `crates/service/src/engine_state.rs`), the
service never touches I/O, and the transport never touches account arithmetic.
Because everything is generic, tests can swap a real backing for a mock at any
seam.

### Storage: an append-only queue and a key-value store

Two contracts, each with an in-memory and a memory-mapped backing:

- **`Queue<T>`** (`crates/storage/src/queue.rs`) — `append(&[T]) -> Offset`
  appends a batch at the tail; `next_batch(Offset) -> &[T]` returns the records
  from that offset as a slice *borrowing the queue's own storage*. Records are
  read in place, never copied out.
- **`Kv<K, V>`** (`crates/storage/src/kv.rs`) — `load(K) -> Option<V>`,
  `next_key(Option<K>)`, `store(K, V)`. `next_key` is the lazy cursor that
  walks clients in ascending order for balance projection.

Both take `&self` and use interior mutability, so a producer and its readers
share one store by reference. The single-threaded runtime means no locks.

The backings:

- **`MemoryQueue`** — records live in segments, one per appended batch,
  allocated at exact length in the caller's `A: Allocator`. A published record
  never moves, which is what lets `next_batch` hand out a borrowing slice. A
  fixed 32-level directory addresses segments by offset.
- **`MemoryKv`** — a `hashbrown::HashMap` in the caller's allocator.
- **`FileQueue`** — a single `mmap` region reserved once at a fixed capacity
  and never remapped. Appends copy records into the tail; `next_batch`
  reinterprets the published range as `&[T]` from the stable base pointer. The
  only `unsafe` is the mapping and the typed reinterpretation of its bytes,
  both confined to this module with `SAFETY` notes.
- **`FileKv`** — an append-only mmap log of `(key, value)` records plus an
  in-memory `HashMap<K, usize>` index of byte offsets. An overwrite appends a
  fresh record and repoints the key, leaving the superseded bytes in place;
  `load` reads the indexed record straight out of the mapping.

The file backings are `TryFrom<Parts>` and reject a record type whose alignment
exceeds the page size rather than guessing. The allocator is an explicit
`A: Allocator` throughout; the CLI names the concrete one (`Global`) and the
library crates stay generic.

### Service: the engine rules

`Consumer::consume` (`crates/service/src/consumer.rs`) drains the transaction
stream into the queue, replays the queue through `Engine::apply`, and returns a
lazy `BalanceStream` that projects one `AccountBalance` per client on demand.
`Transactions` and `Balances` (`crates/service/src/stream.rs`) are slice
streams — `next_batch` returns `Option<&[T]>` — so no materialized row vector
ever exists.

The rules, all overflow-checked and driven by an explicit lifecycle
(`TxKind`/`TxStatus` in `crates/service/src/engine_state.rs`):

- **deposit** credits `available`; a first reference creates the account. A
  reused transaction id keeps the first recorded lifecycle but the later row is
  still credited.
- **withdrawal** records the transaction (even one that will fail), then debits
  `available`; insufficient funds is a no-op that leaves the record in place so
  later events can reference it.
- **dispute** holds the amount (`available` → `held`) only for a `Deposit`
  whose lifecycle is `Available` and whose recorded client matches. Unknown
  tx, a withdrawal, a mismatched client, or an already-disputed deposit is
  ignored.
- **resolve** releases a `Disputed` deposit back to `available`.
- **chargeback** reverses a `Disputed` deposit, releases the hold, and locks
  the account.

Every reference action checks kind, client, and status before moving money, so
the edge cases (dispute a withdrawal, double dispute, dispute after chargeback)
fall out as no-ops instead of special cases. Failed-transaction bookkeeping is
the `TxRecord` itself: a failed withdrawal still exists in the transaction
store.

### Transport: stdio CSV, hand-written

`transport::serve` (`crates/transport/src/stdio.rs`) adapts the CSV reader into
the `Transactions` slice stream, drives the consumer, and writes the projected
balances to stdout.

The reader (`crates/transport/src/reader.rs`) is hand-written, and the record
loop carries a 1-based line number into every `RecordError`: a malformed
amount, an out-of-range id, a missing column, or an unknown `type` fails with
the offending line and the raw field text, and the run aborts (fail-loud,
nothing on stdout).

The stdout sink (`crates/transport/src/stdout.rs`) fixes a real footgun:
compio's `polling` driver registers stdout with epoll, which rejects regular
files with `EPERM`, so shell redirection (`> accounts.csv`) would fail while
piping works. The sink inspects `/proc/self/fd/1`: a regular file is written
positionally through a `File` (no epoll), a pipe or tty keeps the epoll-backed
`Stdout`.

### Correctness and robustness

- **Zero-copy and borrow discipline.** Domain types are borrowed or `Copy`;
  there is no `String`/`Vec` of a domain type in the pipeline. `next_batch`
  and both slice streams return `&[T]`, and the file backings back those
  borrows with a stable, never-remapped mmap.
- **One error type per function.** Every fallible operation owns its `thiserror`
  enum in the same file; errors compose with `#[from]`/`#[source]`, so a
  failure names the operation that produced it end to end.
- **No silent fallbacks.** A malformed row aborts the run. There are no
  `unwrap`/`expect`, no `unwrap_or`-style defaults, no indexing, and no lint
  suppressions — each is banned by a lint rather than left to convention.
- **Cooperative cancellation.** Every operation takes a `&CancellationToken`
  and races `run_until_cancelled`; the CLI selects SIGINT/SIGTERM against the
  pipeline, so a cancelled run shuts down cleanly instead of leaking a partial
  write.
- **Thread-per-core.** The compio runtime is single-threaded; shared stores
  use `RefCell` interior mutability, so there are no mutexes and no lock
  contention in the hot path.

### The lint discipline

`just lint` runs `clippy` with every group denied (including `restriction`)
plus ast-grep rules that enforce one invariant each by construction:

- `no-std-io` / `no-std-fs` — no blocking `std::io`/`std::fs` in library code;
  all I/O is async compio.
- `no-implicit-alloc` — no `Vec::new`/`vec![]`/`.collect`/`format!`/`Box::new`
  in production code; allocation is explicit.
- `no-global-allocator` — library crates never name `Global` and never take an
  allocator by reference; they thread `A: Allocator` by value.
- `no-materialize` — no `Vec<D>`/`Box<[D]>` of a domain type; records stream
  through borrowed slices.
- `no-to-string` / `no-new-get` — no `.to_string()` on domain values; no `new`
  constructor (use `TryFrom`/`From`) and no `get`/`get_*` getter (use
  `derive_more::Deref`).
- `no-static-lifetime` — the `'static` token is banned; real lifetimes are
  named.
- `shared-error-types` — one error enum per fallible function, never reused
  across two operations.
- `no-fallbacks` — no silent fallback, error discard, unchecked arithmetic,
  cloning, or error-source loss.
- `no-suppressions` — no `#[allow]`/`#[expect]` anywhere.
- `panic` — no `unwrap`/`expect`/indexing that can panic.

### Testing

Three layers, all run through `just test` (the `rudzio` runner under a coverage
gate):

- **Unit** — `crates/service/tests` drives the engine against `mockall` doubles
  of the storage traits; `crates/storage` unit-tests the in-memory backings.
- **Integration** — `crates/storage/tests` exercises the real file/mmap
  backings against the filesystem, including capacity and cancellation paths.
- **End-to-end** — `tests/e2e` runs the compiled binary over 89 fixture cases
  (`tests/e2e/fixtures/<NNN-name>/`, each with `input.csv`, `expected.csv`,
  `case.toml`): the spec example, every transaction kind, boundary ids (`u16`/
  `u32` max and zero), whitespace and decimal precision, header-only input, and
  the malformed-row cases. Output is compared as a **multiset** — row order is
  unspecified but multiplicity matters — with value-based `Decimal` equality,
  so `1.5` matches `1.5000`.

Run `just lint` and `just test`; both are wired into the pre-commit hook.

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
| `crates/storage` | persistence only: append-only queue and key-value state | `derive_more`, `thiserror` |
| `crates/service` | all business rules, generic over the storage traits | `domain`, `storage`, `thiserror`, `tokio-util` |
| `crates/transport` | stdio CSV reading and writing | `compio`, `domain`, `rust_decimal`, `service`, `thiserror`, `tokio-util` |
| `crates/cli` | composition root: builds real storage, service and transport and wires them | `compio`, `domain`, `pico-args`, `service`, `storage`, `transport`, `thiserror`, `tokio-util` |

Shared workspace dependencies, pinned to exact versions: `anyhow` 1.0.104
(dev), `compio` 0.19.2, `csv` 1.4.0, `mockall` 0.15.0 (dev, `service` only),
`pico-args` 0.5.0, `rust_decimal` 1.43.0, `serde` 1.0.229, `thiserror` 2.0.21,
`tokio-util` 0.7.20, and `rudzio` (git, the test runner).

### Layering and trait contracts

Every contract method returns its own error type, so a failure names the
operation that produced it. Service stages take a `&CancellationToken`; worker
jobs run on a `tokio_util::task::TaskTracker`.

**Storage** (`crates/storage`) has exactly two parts and decides nothing. It is
tested by integration tests in `crates/storage/tests/` against the real
backing; it has no mocks.

| part | trait | method |
| --- | --- | --- |
| queue | `Append<T: Copy>` | `append(&self, batch: &[T]) -> Span` appends after the tail |
| queue | `ReadAt<T: Copy>` | `read(&self, from: Position, into: &mut [T]) -> Span` copies into a caller-owned buffer |
| kv | `Lookup<K: Copy, V: Copy>` | `read(&self, key: K) -> Option<V>` |
| kv | `Update<K: Copy, V: Copy>` | `update(&self, key: K, value: &V)` |
| kv | `Walk<K: Copy, V: Copy>` | `next_entry(&self, from: Slot) -> Option<Entry<K, V>>`, a lazy cursor walk |

`Queue<T>` and `Kv<K, V>` are blanket umbrella traits over those parts.
`Position` is a record index (a mmap backing derives the byte offset as
`position * size_of::<T>()`); `Span` is a half-open `[start, end)` range of
positions. Records and values are fixed-size `Copy` types so a memory-mapped
backing stores them as slots. Methods take `&self` so a producer, readers and
concurrent workers share one store by reference; a backing synchronises
internally. `MmapQueue` and `MmapKv` are the production backings; every method
returns a typed `Unimplemented` until the mmap store lands.

**Service** (`crates/service`) owns every rule: deposit, withdrawal, dispute,
resolve, chargeback, routing of clients to worker jobs, and failed-transaction
bookkeeping. `Engine<'store, Q, K> { queue: &Q, kv: &K }` borrows both stores
and never names a backing. Each stage bounds only the operations it uses:

| stage | bound | signature |
| --- | --- | --- |
| ingest | `Q: Append<Transaction>` | `ingest(&self, source: &mut impl TransactionStream, batch: &mut [Transaction], &CancellationToken) -> Span` |
| aggregate | `Q: ReadAt<Transaction>`, `K: Lookup + Update` | `aggregate(&self, from: Position, ingested: Span, batch: &mut [Transaction], &TaskTracker, &CancellationToken)` |
| balances | `K: Walk` | `balances(&self) -> Balances<'store, K>`, a lazy `BalanceStream` |

Every stage body is a stub returning a typed `Unimplemented`. Unit tests in
`crates/service/tests/` drive each stage against `mockall` doubles of the
queue and kv traits.

**Transport** (`crates/transport`) implements `TransactionStream` (the CSV
`Reader`) and writes balances (`Writer`) over stdio or a file. There is one real
implementation, and the traits leave room for other protocols.

**CLI** (`crates/cli`) is the composition root. It constructs `MmapQueue` and
`MmapKv`, lends them to `Engine`, and runs ingest, aggregate and balances over
the transport reader and writer.

### Open contract questions

- **Duplicate transaction ids.** No dedicated storage part. Options: (a) the
  service keeps a `Kv<TxId, TxRecord>` of seen ids, which it needs anyway for
  disputes; (b) ingest filters before append using a lookup on that store;
  (c) aggregate rejects a duplicate when applying it, recording it as a failed
  transaction. (a)+(c) keeps the queue a faithful log of input.
- **End of input.** No dedicated signal. Options: (a) the ingest stage
  completing and returning the ingested `Span` is the signal (current
  contract: aggregate runs after ingest); (b) aggregate runs concurrently and
  tails the queue, so it needs a way to learn the final end, e.g. a sentinel
  `Transaction::Eof` record appended last.
- **Kv value type.** The output row `AccountBalance` cannot hold disputed
  transaction amounts. The service likely defines its own fixed-size client
  record and a separate per-transaction record; storage stays generic over `V`.
- **Position semantics.** Positions are record indices from queue start and
  never reused; whether a backing needs a committed-position marker for
  restart is undecided.

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

The eleven cases the spec left open are now resolved using the provisional
engine behaviour as the decision. Each is recorded with its decided behaviour
and the `file:line` of the decision under
[Open decisions for review](#open-decisions-for-review); the fixtures carry
`outcome = "success"` with a checked-in `expected.csv`.

## Open decisions for review

Resolved decisions, one per previously-open case: each records the decided
behaviour and the `file:line` of the engine logic that implements it. All
eleven fixtures now run to completion (`outcome = "success"`) against a
checked-in `expected.csv`.

- **`failed-withdrawal-new-client`** — A withdrawal against a client with no
  record creates the record (spec: "If a client doesn't exist create a new
  record"), then fails for insufficient funds, leaving the record at zero; the
  client is emitted with zero balances.
  - Decided in: `crates/service/src/consumer.rs:273`.
- **`dispute-withdrawal`** — A withdrawal is now retained in the transaction
  store (info kept), but disputing it is still ignored: dispute is only
  meaningful for deposits, and the referenced withdrawal is detected by its
  `kind`.
  - Decided in: `crates/service/src/consumer.rs:342`.
- **`dispute-insufficient-available`** — A dispute proceeds unclamped: the
  `checked_sub` drives `available` negative when the disputed amount exceeds it.
  - Decided in: `crates/service/src/consumer.rs:330`.
- **`locked-account-operations`** — Locking does not gate later operations;
  deposits and withdrawals on a locked account still apply (no `locked()` check).
  - Decided in: `crates/service/src/consumer.rs:205-294`.
- **`mismatched-client-reference`** — The referenced transaction's client is
  authoritative; a dispute/resolve/chargeback naming a different client is
  ignored.
  - Decided in: `crates/service/src/consumer.rs:304`.
- **`duplicate-tx-id`** — A reused transaction id is applied to the new client
  (amount credited) while the first record is kept: the later `store_tx` is
  skipped, but the later row is still applied.
  - Decided in: `crates/service/src/consumer.rs:207-227`.
- **`redispute-after-resolve`** — Resolve restores the deposit to `Available`,
  so a later dispute re-holds the funds.
  - Decided in: `crates/service/src/consumer.rs:350-404`.
- **`dispute-after-chargeback`** — Chargeback leaves the deposit `ChargedBack`;
  a later dispute is ignored.
  - Decided in: `crates/service/src/consumer.rs:304`.
- **`double-dispute`** — A second dispute of an already-`Disputed` deposit is
  ignored (funds are not held twice).
  - Decided in: `crates/service/src/consumer.rs:304`.
- **`dispute-failed-withdrawal`** — Disputing a withdrawal is ignored whether it
  succeeded or failed: the withdrawal is retained in the transaction store
  (info kept), but a dispute of a withdrawal is detected by its `kind` and
  ignored.
  - Decided in: `crates/service/src/consumer.rs:342`.
- **`ignored-row-client-output`** — A client appearing only in ignored
  dispute/resolve/chargeback rows is omitted (those arms return early without
  creating account state).
  - Decided in: `crates/service/src/consumer.rs:302,357,412`.

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
