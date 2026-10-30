//! The transaction-source contract: a fallible, cancel-aware, batched stream
//! of transactions.

use record_processor_domain::input::Transaction;
use tokio_util::sync::CancellationToken;

/// One batch of transactions, borrowed zero-copy from the source's reused
/// buffers.
///
/// `raw` carries the exact CSV bytes of every row in the batch (so a consumer
/// can persist them without copying); `transactions` is the parsed view of the
/// same rows.
#[derive(Debug)]
pub struct TransactionBatch<'batch> {
    /// The raw CSV bytes of this batch's rows.
    pub raw: &'batch [u8],
    /// The parsed transactions of this batch.
    pub transactions: &'batch [Transaction],
}

/// A source of transactions, pulled one batch at a time.
///
/// A source yields [`Transaction::Eof`] as the sole transaction of its final
/// batch immediately before terminating, so a consumer can distinguish a
/// normal end-of-input from a mid-stream failure. Cancellation is cooperative:
/// the caller passes a [`CancellationToken`]; once it fires the stream yields a
/// typed `Cancelled` error item.
pub trait TransactionStream {
    /// The error produced while pulling the next batch.
    type Error: core::fmt::Debug + core::fmt::Display;

    /// Pull the next batch, or `None` once the stream is exhausted.
    async fn next_batch(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<TransactionBatch<'_>>, Self::Error>;
}
