//! The append-only queue contract: append batches of records and read them
//! back as borrowed slices of the queue's own storage.

use core::error::Error;

use tokio_util::sync::CancellationToken;

use crate::offset::Offset;

/// An append-only, unbounded queue of `T` records.
///
/// Takes `&self` so a producer and consumers share the queue by reference;
/// the backing uses interior mutability, relying on the single-threaded
/// runtime for safety.
pub trait Queue<T> {
    /// The error produced by [`Queue::append`].
    type AppendError: Error;
    /// The error produced by [`Queue::next_batch`].
    type NextBatchError: Error;

    /// Append every record of `batch` after the current tail, returning the
    /// offset of the first appended record.
    fn append(
        &self,
        batch: &[T],
        cancel: &CancellationToken,
    ) -> impl Future<Output = Result<Offset, Self::AppendError>>;

    /// Return the records starting at `since`, waiting until the record at
    /// `since` has been appended. The slice borrows the queue's storage, so
    /// consumers read records in place.
    fn next_batch<'queue>(
        &'queue self,
        since: Offset,
        cancel: &CancellationToken,
    ) -> impl Future<Output = Result<&'queue [T], Self::NextBatchError>>
    where
        T: 'queue;
}
