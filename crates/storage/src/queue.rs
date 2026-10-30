//! The append-only queue contract: unbounded appends of batches and a
//! zero-copy read of the next batch.

// TODO(lint/E0433): cannot find module or crate `tokio_util` in this scope
use tokio_util::sync::CancellationToken;

// TODO(lint/E0432): unresolved import `crate::offset`
use crate::offset::Offset;

/// An append-only, unbounded queue of batches of `T` records.
///
/// Batches are read back as borrowed slices tied to the queue borrow, so a
/// consumer keeps the slice alive for as long as it keeps the queue alive and
/// never copies a batch out of the queue.
// TODO(lint/unexpected_cfgs): unexpected `cfg` condition value: `mocks`
#[cfg_attr(feature = "mocks", automock)]
pub trait Queue<T> {
    /// The error produced by [`Queue::append`] and [`Queue::next_batch`].
    type Error: core::error::Error;

    /// Append every record of `batch`, returning the offset just past it.
    async fn append(&self, batch: &[T], cancel: &CancellationToken) -> Result<Offset, Self::Error>;

    /// Return the next appended batch at or after `since`, blocking until one
    /// is available when `since` has reached the queue tail.
    async fn next_batch(
        &self,
        since: Offset,
        cancel: &CancellationToken,
    ) -> Result<&[T], Self::Error>;
}
