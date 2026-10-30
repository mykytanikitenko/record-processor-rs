//! The service contract and the engine that owns all business rules, generic
//! over the storage traits so it can be unit-tested against mocks.

use storage::{DedupIndex, EndOfInput, EventQueue, KvState};
use tokio_util::sync::CancellationToken;

use crate::balance_stream::BalanceStream;
use crate::serve::EngineError;
use crate::transaction_stream::TransactionStream;

/// A service consuming a transaction stream and producing a lazy stream of
/// per-client balances.
pub trait Service {
    /// The error produced while serving.
    type Error: core::fmt::Debug + core::fmt::Display;
    /// The lazy balance stream produced once input is consumed.
    type Output: BalanceStream;

    /// Consume `input`, returning the balance stream.
    async fn serve<S>(
        self,
        input: S,
        cancel: &CancellationToken,
    ) -> Result<Self::Output, Self::Error>
    where
        S: TransactionStream;
}

/// Failure to serve through the engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ServeError {
    /// The engine's own serve implementation failed.
    #[error(transparent)]
    Impl(#[from] EngineError),
}

/// The payments engine, owning one handle per storage component so each can be
/// backed independently (the real store in production, a mock in unit tests).
#[derive(Debug)]
pub struct Engine<Q, I, E, K> {
    /// End-of-input signal between ingest and workers.
    pub eof: E,
    /// Deduplication index of seen transaction ids.
    pub index: I,
    /// Append-only queue of accepted events.
    pub queue: Q,
    /// Per-client account state.
    pub state: K,
}

impl<Q, I, E, K> Service for Engine<Q, I, E, K>
where
    Q: EventQueue,
    I: DedupIndex,
    E: EndOfInput,
    K: KvState,
{
    type Error = ServeError;
    type Output = crate::balance_stream::RealBalances;

    #[inline]
    async fn serve<S>(
        self,
        input: S,
        cancel: &CancellationToken,
    ) -> Result<Self::Output, Self::Error>
    where
        S: TransactionStream,
    {
        self.serve_impl(input, cancel).await.map_err(Into::into)
    }
}
