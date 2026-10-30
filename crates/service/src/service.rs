//! The service contract: consume a transaction stream, produce account
//! balances, dispatched between the real engine and a unit-test mock.

use tokio_util::sync::CancellationToken;

use crate::balance_stream::Balances;
use crate::serve::EngineError;
use crate::transaction_stream::TransactionStream;

/// A payments engine that consumes a transaction stream and produces the final
/// per-client account balances as a lazy, owned stream.
pub trait Service {
    /// The error produced while serving.
    type Error: core::fmt::Debug + core::fmt::Display;

    /// Consume `input` to completion and produce the account balances.
    ///
    /// Cancellation is cooperative via `cancel`: the returned stream yields a
    /// typed `Cancelled` item once the token fires.
    async fn serve<S>(self, input: S, cancel: &CancellationToken) -> Result<Balances, Self::Error>
    where
        S: TransactionStream;
}

/// Failure to serve through the production engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ServeError {
    /// The engine's own serve implementation failed.
    #[error(transparent)]
    Impl(#[from] EngineError),
}

/// Failure to serve through the unit-test mock.
#[cfg(feature = "unit")]
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum MockServiceError {
    /// The mock has no implementation.
    #[error("mock service is not implemented")]
    Unimplemented,
}

/// Failure to serve through a dispatched service.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ServeDispatchError {
    /// The unit-test mock failed.
    #[cfg(feature = "unit")]
    #[error(transparent)]
    Mock(#[from] MockServiceError),
    /// The production engine failed.
    #[error(transparent)]
    Real(#[from] ServeError),
}

/// The production engine (a contract stub until the business logic lands).
#[derive(Clone, Copy, Debug)]
pub struct Engine;

impl Service for Engine {
    type Error = ServeError;

    #[inline]
    async fn serve<S>(self, input: S, cancel: &CancellationToken) -> Result<Balances, Self::Error>
    where
        S: TransactionStream,
    {
        self.serve_impl(input, cancel).await.map_err(Into::into)
    }
}

/// The unit-test mock, returning a canned failure without running the engine.
#[cfg(feature = "unit")]
#[derive(Clone, Copy, Debug)]
pub struct Mock;

#[cfg(feature = "unit")]
impl Service for Mock {
    type Error = MockServiceError;

    #[inline]
    async fn serve<S>(self, _input: S, _cancel: &CancellationToken) -> Result<Balances, Self::Error>
    where
        S: TransactionStream,
    {
        Err(MockServiceError::Unimplemented)
    }
}

/// Dispatch between the real engine and the unit-test mock.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub enum Type {
    /// The unit-test mock.
    #[cfg(feature = "unit")]
    Mock(Mock),
    /// The production engine.
    Real(Engine),
}

impl Service for Type {
    type Error = ServeDispatchError;

    #[inline]
    async fn serve<S>(self, input: S, cancel: &CancellationToken) -> Result<Balances, Self::Error>
    where
        S: TransactionStream,
    {
        match self {
            #[cfg(feature = "unit")]
            Self::Mock(mock) => mock.serve(input, cancel).await.map_err(Into::into),
            Self::Real(engine) => engine.serve(input, cancel).await.map_err(Into::into),
        }
    }
}
