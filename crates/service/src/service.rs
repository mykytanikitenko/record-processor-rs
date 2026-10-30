//! The service contract: consume a transaction stream, produce account
//! balances.

use tokio_util::sync::CancellationToken;

use crate::balance_stream::RealBalances;
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
    async fn serve<S>(
        self,
        input: S,
        cancel: &CancellationToken,
    ) -> Result<RealBalances, Self::Error>
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

/// The production engine (a contract stub until the business logic lands).
#[derive(Clone, Copy, Debug)]
pub struct Engine;

impl Service for Engine {
    type Error = ServeError;

    #[inline]
    async fn serve<S>(
        self,
        input: S,
        cancel: &CancellationToken,
    ) -> Result<RealBalances, Self::Error>
    where
        S: TransactionStream,
    {
        self.serve_impl(input, cancel).await.map_err(Into::into)
    }
}
