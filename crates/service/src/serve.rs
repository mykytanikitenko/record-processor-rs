//! The engine's `serve` implementation; no business logic yet, so serving
//! fails with [`EngineError::Unimplemented`].

use storage::{DedupIndex, EndOfInput, EventQueue, KvState};
use tokio_util::sync::CancellationToken;

use crate::balance_stream::RealBalances;
use crate::service::Engine;
use crate::transaction_stream::TransactionStream;

/// Failure inside the engine's serve implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum EngineError {
    /// The engine has no business-logic implementation yet.
    #[error("service engine is not implemented")]
    Unimplemented,
}

impl<Q, I, E, K> Engine<Q, I, E, K>
where
    Q: EventQueue,
    I: DedupIndex,
    E: EndOfInput,
    K: KvState,
{
    /// Serve `input`, producing the lazy balance stream.
    pub(crate) async fn serve_impl<S>(
        self,
        _input: S,
        _cancel: &CancellationToken,
    ) -> Result<RealBalances, EngineError>
    where
        S: TransactionStream,
    {
        Err(EngineError::Unimplemented)
    }
}
