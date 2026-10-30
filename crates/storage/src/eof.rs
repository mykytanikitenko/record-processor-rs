//! The end-of-input signal contract.

/// Failure to use the end-of-input signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum EndOfInputError {
    /// The signal is a contract stub with no implementation yet.
    #[error("end-of-input signal is not implemented")]
    Unimplemented,
}

/// A one-shot signal that the input stream has been fully consumed.
#[mockall::automock(type Error = EndOfInputError;)]
pub trait EndOfInput {
    /// The error produced by the signal.
    type Error: core::fmt::Debug + core::fmt::Display;

    /// Signal that input has ended.
    async fn notify(&mut self) -> Result<(), Self::Error>;

    /// Wait until input has ended.
    async fn wait(&self) -> Result<(), Self::Error>;
}
