//! Slice-stream contracts: batch sources of transactions and balances.

use core::error::Error;

use record_processor_domain::input::Transaction;
use record_processor_domain::output::AccountBalance;
use tokio_util::sync::CancellationToken;

/// A source of transaction batches, read in place from the backing storage.
///
/// A batch borrows the stream's own storage, so callers read records without
/// copying them. Cancellation is cooperative: the caller passes a
/// [`CancellationToken`]; once it fires the stream yields a typed error item.
pub trait Transactions {
    /// The error produced while reading the next batch.
    type Error: Error;

    /// Return the next batch of transactions, or `None` once the stream is
    /// exhausted.
    async fn next_batch<'stream>(
        &'stream self,
        cancel: &CancellationToken,
    ) -> Result<Option<&'stream [Transaction]>, Self::Error>;
}

/// A source of account-balance batches, read in place from the backing
/// storage.
///
/// A batch borrows the stream's own storage, so callers read rows without
/// copying them. Balances are computed on demand rather than accumulated
/// upfront, so advancing the stream mutates its cursor: `next_batch` takes
/// `&mut self`. Cancellation is cooperative: the caller passes a
/// [`CancellationToken`]; once it fires the stream yields a typed error item.
pub trait Balances {
    /// The error produced while reading the next batch.
    type Error: Error;

    /// Return the next batch of balances, or `None` once the stream is
    /// exhausted.
    async fn next_batch<'stream>(
        &'stream mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<&'stream [AccountBalance]>, Self::Error>;
}
