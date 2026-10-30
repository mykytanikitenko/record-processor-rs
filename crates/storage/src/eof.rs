//! The end-of-input signal contract.

use tokio_util::sync::CancellationToken;

/// Failure to raise the end-of-input signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum SignalNotifyError {
    /// The notification was cancelled before it was recorded.
    #[error("end-of-input notify cancelled")]
    Cancelled,
    /// The signal is a contract stub with no backing store yet.
    #[error("end-of-input notify is not implemented")]
    Unimplemented,
}

/// Failure to wait for the end-of-input signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum SignalWaitError {
    /// The wait was cancelled before input ended.
    #[error("end-of-input wait cancelled")]
    Cancelled,
    /// The signal is a contract stub with no backing store yet.
    #[error("end-of-input wait is not implemented")]
    Unimplemented,
}

/// A one-shot signal carrying the id one past the last appended event, so a
/// consumer knows when it has drained the whole queue.
pub trait EndOfInput {
    /// The error produced by [`EndOfInput::notify`].
    type NotifyEndError: core::fmt::Debug + core::fmt::Display;
    /// The error produced by [`EndOfInput::wait`].
    type WaitEndError: core::fmt::Debug + core::fmt::Display;

    /// Record that input ended with `end` as the queue's exclusive end id.
    async fn notify(
        &mut self,
        end: u64,
        cancel: &CancellationToken,
    ) -> Result<(), Self::NotifyEndError>;

    /// Wait until input has ended, returning the queue's exclusive end id.
    async fn wait(&mut self, cancel: &CancellationToken) -> Result<u64, Self::WaitEndError>;
}
