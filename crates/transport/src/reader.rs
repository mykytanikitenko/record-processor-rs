//! The transaction reader contract: an owned stream over an async byte source,
//! dispatched between the real reader and a unit-test mock.

use record_processor_domain::input::Transaction;
use service::TransactionStream;
use tokio_util::sync::CancellationToken;

/// Failure to read from the production reader.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ReadError {
    /// The stream was cancelled before it exhausted.
    #[error("transaction reader cancelled")]
    Cancelled,
    /// The reader is a contract stub with no implementation yet.
    #[error("transaction reader is not implemented")]
    Unimplemented,
}

/// Failure to read from the mock reader.
#[cfg(feature = "unit")]
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum MockReadError {
    /// The stream was cancelled before it exhausted.
    #[error("transaction reader cancelled")]
    Cancelled,
}

/// Failure to read from a dispatched reader.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ReadDispatchError {
    /// The mock reader failed.
    #[cfg(feature = "unit")]
    #[error(transparent)]
    Mock(#[from] MockReadError),
    /// The production reader failed.
    #[error(transparent)]
    Real(#[from] ReadError),
}

/// A production transaction reader over an async byte source.
#[derive(Debug)]
pub struct Reader<S> {
    /// The async byte source the real reader will consume.
    source: S,
}

impl<S> From<S> for Reader<S> {
    #[inline]
    fn from(source: S) -> Self {
        Self { source }
    }
}

impl<S> Reader<S> {
    /// Consume the reader, returning the underlying source.
    #[inline]
    pub fn into_inner(self) -> S {
        self.source
    }
}

impl<S> TransactionStream for Reader<S> {
    type Error = ReadError;

    #[inline]
    async fn next(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<Transaction>, Self::Error> {
        if cancel.is_cancelled() {
            Err(ReadError::Cancelled)
        } else {
            Err(ReadError::Unimplemented)
        }
    }
}

/// The mock transaction reader, enabled for unit tests only. It yields at most
/// one transaction (set at construction) and then terminates.
#[cfg(feature = "unit")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mock {
    /// The single transaction to yield, then `None`.
    item: Option<Transaction>,
}

#[cfg(feature = "unit")]
impl From<Option<Transaction>> for Mock {
    #[inline]
    fn from(item: Option<Transaction>) -> Self {
        Self { item }
    }
}

#[cfg(feature = "unit")]
impl TransactionStream for Mock {
    type Error = MockReadError;

    #[inline]
    async fn next(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<Transaction>, Self::Error> {
        if cancel.is_cancelled() {
            Err(MockReadError::Cancelled)
        } else {
            Ok(self.item.take())
        }
    }
}

/// Dispatch between a real reader and a mock reader.
#[non_exhaustive]
#[derive(Debug)]
pub enum Type<S> {
    /// The mock reader, enabled for unit tests only.
    #[cfg(feature = "unit")]
    Mock(Mock),
    /// The production reader.
    Real(Reader<S>),
}

impl<S> TransactionStream for Type<S> {
    type Error = ReadDispatchError;

    #[inline]
    async fn next(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<Transaction>, Self::Error> {
        match self {
            #[cfg(feature = "unit")]
            Self::Mock(inner) => inner.next(cancel).await.map_err(Into::into),
            Self::Real(inner) => inner.next(cancel).await.map_err(Into::into),
        }
    }
}
