//! The engine's `serve` implementation, decomposed from the trait method.
//!
//! Contracts only: no business logic is implemented yet, so serving always
//! fails with [`EngineError::Unimplemented`].

use tokio_util::sync::CancellationToken;

use crate::balance_stream::Balances;
use crate::service::Engine;
use crate::transaction_stream::TransactionStream;

/// Failure to serve a transaction stream (the engine's own implementation
/// error, before it is wrapped by [`crate::service::ServeError`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum EngineError {
    /// The engine has no business-logic implementation yet.
    #[error("service engine is not implemented")]
    Unimplemented,
}

impl Engine {
    /// Serve `input`, producing a lazy balance stream.
    ///
    /// A contract stub: always fails with [`EngineError::Unimplemented`].
    pub(crate) async fn serve_impl<S>(
        self,
        _input: S,
        _cancel: &CancellationToken,
    ) -> Result<Balances, EngineError>
    where
        S: TransactionStream,
    {
        Err(EngineError::Unimplemented)
    }
}
