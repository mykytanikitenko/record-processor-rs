//! The memory-mapped queue backing (a contract stub until the backing lands).

// TODO(lint/E0432): unresolved imports `crate::queue::Append`, `crate::queue::Position`, `crate::queue::ReadAt`, `crate::queue::Span`
use crate::queue::{Append, Position, ReadAt, Span};

/// Failure to append to the memory-mapped queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum AppendError {
    /// The backing has no implementation yet.
    #[error("memory-mapped queue append is not implemented")]
    Unimplemented,
}

/// Failure to read from the memory-mapped queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ReadAtError {
    /// The backing has no implementation yet.
    #[error("memory-mapped queue read is not implemented")]
    Unimplemented,
}

/// The memory-mapped queue backing. Every method returns its typed
/// `Unimplemented` error.
#[derive(Clone, Copy, Debug)]
pub struct MmapQueue;

impl<T> Append<T> for MmapQueue
where
    T: Copy,
{
    type Error = AppendError;

    #[inline]
    async fn append(&self, _batch: &[T]) -> Result<Span, Self::Error> {
        Err(AppendError::Unimplemented)
    }
}

impl<T> ReadAt<T> for MmapQueue
where
    T: Copy,
{
    type Error = ReadAtError;

    #[inline]
    async fn read(&self, _from: Position, _into: &mut [T]) -> Result<Span, Self::Error> {
        Err(ReadAtError::Unimplemented)
    }
}
