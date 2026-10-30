//! The account-balance writer contract: an owned sink over an async byte
//! target, dispatched between the real writer and a unit-test mock.

use compio::io::AsyncWrite;
use record_processor_domain::output::AccountBalance;
use tokio_util::sync::CancellationToken;

/// Failure to write through the production writer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum WriteError {
    /// The sink was cancelled before every balance was written.
    #[error("balance writer cancelled")]
    Cancelled,
    /// The writer is a contract stub with no implementation yet.
    #[error("balance writer is not implemented")]
    Unimplemented,
}

/// Failure to write through the mock writer.
#[cfg(feature = "unit")]
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum MockWriteError {
    /// The sink was cancelled before every balance was written.
    #[error("balance writer cancelled")]
    Cancelled,
}

/// Failure to write through a dispatched writer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum WriteDispatchError {
    /// The mock writer failed.
    #[cfg(feature = "unit")]
    #[error(transparent)]
    Mock(#[from] MockWriteError),
    /// The production writer failed.
    #[error(transparent)]
    Real(#[from] WriteError),
}

/// A production account-balance writer over an async byte sink.
#[derive(Debug)]
pub struct Writer<S> {
    /// The async byte sink the real writer will target.
    sink: S,
}

impl<S> From<S> for Writer<S> {
    #[inline]
    fn from(sink: S) -> Self {
        Self { sink }
    }
}

impl<S> Writer<S> {
    /// Consume the writer, returning the underlying sink.
    #[inline]
    pub fn into_inner(self) -> S {
        self.sink
    }
}

impl<S> Writer<S>
where
    S: AsyncWrite,
{
    /// Write every balance to the sink.
    ///
    /// # Errors
    ///
    /// A contract stub: fails with [`WriteError::Unimplemented`] until the real
    /// CSV framing lands, or with [`WriteError::Cancelled`] when the token
    /// fires.
    #[inline]
    pub async fn write_all(
        &mut self,
        _balances: &[AccountBalance],
        cancel: &CancellationToken,
    ) -> Result<(), WriteError> {
        if cancel.is_cancelled() {
            Err(WriteError::Cancelled)
        } else {
            Err(WriteError::Unimplemented)
        }
    }
}

/// The mock account-balance writer, enabled for unit tests only. It discards
/// every balance and reports success.
#[cfg(feature = "unit")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mock;

#[cfg(feature = "unit")]
impl Mock {
    /// Accept the balances and report success without writing anything.
    ///
    /// # Errors
    ///
    /// Returns [`MockWriteError::Cancelled`] when the token fires.
    #[inline]
    pub async fn write_all(
        &mut self,
        _balances: &[AccountBalance],
        cancel: &CancellationToken,
    ) -> Result<(), MockWriteError> {
        if cancel.is_cancelled() {
            Err(MockWriteError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Dispatch between a real writer and a mock writer.
#[non_exhaustive]
#[derive(Debug)]
pub enum Type<S> {
    /// The mock writer, enabled for unit tests only.
    #[cfg(feature = "unit")]
    Mock(Mock),
    /// The production writer.
    Real(Writer<S>),
}

impl<S> Type<S>
where
    S: AsyncWrite,
{
    /// Write every balance to the sink, dispatching on the variant.
    ///
    /// # Errors
    ///
    /// Returns the inner variant's error.
    #[inline]
    pub async fn write_all(
        &mut self,
        balances: &[AccountBalance],
        cancel: &CancellationToken,
    ) -> Result<(), WriteDispatchError> {
        match self {
            #[cfg(feature = "unit")]
            Self::Mock(inner) => inner.write_all(balances, cancel).await.map_err(Into::into),
            Self::Real(inner) => inner.write_all(balances, cancel).await.map_err(Into::into),
        }
    }
}
