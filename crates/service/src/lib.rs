//! Payments-engine business logic over the storage contracts: ingest the
//! transaction stream into the queue, aggregate the queue into per-client
//! state, and stream the final balances.

pub mod aggregation;
pub mod balance_stream;
pub mod balances;
pub mod engine;
pub mod ingestion;
pub mod transaction_stream;

pub use aggregation::{AggregateError, LookupFailure, ReadFailure, UpdateFailure};
pub use balance_stream::BalanceStream;
pub use balances::{Balances, NextBalanceError, WalkFailure};
pub use engine::Engine;
pub use ingestion::{AppendFailure, IngestError, SourceFailure};
pub use transaction_stream::TransactionStream;
