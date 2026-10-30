//! The append-only event queue contract.

use record_processor_domain::input::Transaction;

/// Failure to use the event queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum QueueError {
    /// The event queue is a contract stub with no implementation yet.
    #[error("event queue is not implemented")]
    Unimplemented,
}

/// An append-only queue of events, each assigned a monotonic id on append.
#[mockall::automock(type Error = QueueError;)]
pub trait EventQueue {
    /// The error produced by the queue.
    type Error: core::fmt::Debug + core::fmt::Display;

    /// Append `event` and return its id.
    async fn append(&mut self, event: &Transaction) -> Result<u32, Self::Error>;

    /// Read the next batch into the caller-provided buffer, returning the
    /// number of bytes written.
    async fn read_batch(&mut self, batch: &mut [u8]) -> Result<usize, Self::Error>;
}
