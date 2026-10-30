//! The append-only event queue contract: batches of transactions persisted in
//! arrival order, each event addressed by a monotonic id.

use record_processor_domain::input::Transaction;
use tokio_util::sync::CancellationToken;

/// The half-open id range `[start, end)` assigned to one appended batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdRange {
    /// One past the last id assigned to the batch.
    pub end: u64,
    /// The first id assigned to the batch.
    pub start: u64,
}

/// Failure to append a batch to the event queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum QueueAppendError {
    /// The append was cancelled before the batch was persisted.
    #[error("event queue append cancelled")]
    Cancelled,
    /// The event queue is a contract stub with no backing store yet.
    #[error("event queue append is not implemented")]
    Unimplemented,
}

/// Failure to read a batch back from the event queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum QueueReadError {
    /// The read was cancelled before the batch was loaded.
    #[error("event queue read cancelled")]
    Cancelled,
    /// The event queue is a contract stub with no backing store yet.
    #[error("event queue read is not implemented")]
    Unimplemented,
}

/// An append-only queue of transactions, written and read in whole batches so
/// one composed IO call serves many events.
pub trait EventQueue {
    /// The error produced by [`EventQueue::append_batch`].
    type AppendBatchError: core::fmt::Debug + core::fmt::Display;
    /// The error produced by [`EventQueue::read_batch`].
    type ReadBatchError: core::fmt::Debug + core::fmt::Display;

    /// Persist `events` in order and return the id range assigned to them.
    async fn append_batch(
        &mut self,
        events: &[Transaction],
        cancel: &CancellationToken,
    ) -> Result<IdRange, Self::AppendBatchError>;

    /// Load events starting at id `from` into the caller-owned `events`
    /// buffer, returning how many slots were filled (zero once `from` reaches
    /// the end of the queue).
    async fn read_batch(
        &mut self,
        from: u64,
        events: &mut [Transaction],
        cancel: &CancellationToken,
    ) -> Result<usize, Self::ReadBatchError>;
}
