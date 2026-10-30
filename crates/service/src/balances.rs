//! Stage three: stream the final per-client state as account balances.

use record_processor_domain::client_id::ClientId;
use record_processor_domain::output::AccountBalance;
use storage::{Slot, Walk};
use tokio_util::sync::CancellationToken;

use crate::balance_stream::BalanceStream;
use crate::engine::Engine;

/// The error of walking the per-client state `K`.
pub type WalkFailure<K> = <K as Walk<ClientId, AccountBalance>>::Error;

/// Failure to pull the next account balance.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum NextBalanceError<E> {
    /// The stream was cancelled before it exhausted.
    #[error("balance stream cancelled")]
    Cancelled,
    /// Walking the per-client state failed.
    #[error("walking client state failed: {0}")]
    State(E),
    /// The stage has no business-logic implementation yet.
    #[error("balance stream is not implemented")]
    Unimplemented,
}

/// A lazy stream of account balances walking the per-client state from
/// `cursor`.
#[derive(Clone, Copy, Debug)]
pub struct Balances<'store, K> {
    /// The cursor of the next entry to yield.
    pub cursor: Slot,
    /// The per-client state store being walked.
    pub kv: &'store K,
}

impl<'store, Q, K> Engine<'store, Q, K> {
    /// Stream every stored client balance, starting from the first entry.
    #[inline]
    #[must_use]
    pub const fn balances(&self) -> Balances<'store, K> {
        Balances {
            cursor: Slot::START,
            kv: self.kv,
        }
    }
}

impl<K> BalanceStream for Balances<'_, K>
where
    K: Walk<ClientId, AccountBalance>,
{
    type Error = NextBalanceError<WalkFailure<K>>;

    #[inline]
    async fn next(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<AccountBalance>, Self::Error> {
        if cancel.is_cancelled() {
            Err(NextBalanceError::Cancelled)
        } else {
            Err(NextBalanceError::Unimplemented)
        }
    }
}
