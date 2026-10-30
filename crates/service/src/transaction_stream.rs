//! The transaction-source contract: a fallible, cancel-aware stream of
//! transactions.

use record_processor_domain::input::Transaction;
use tokio_util::sync::CancellationToken;

/// A source of transactions, pulled one at a time.
///
/// A source yields [`Transaction::Eof`] immediately before terminating so a
/// consumer can distinguish a normal end-of-input from a mid-stream failure.
/// Cancellation is cooperative: the caller passes a [`CancellationToken`];
/// once it fires the stream yields a typed `Cancelled` error item.
pub trait TransactionStream {
    /// The error produced while pulling the next transaction.
    type Error: core::fmt::Debug + core::fmt::Display;

    /// Pull the next transaction, or `None` once the stream is exhausted.
    async fn next(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<Transaction>, Self::Error>;
}
