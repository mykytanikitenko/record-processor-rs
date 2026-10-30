//! Stage two: apply the queue to per-client state through worker jobs.
//!
//! Routing of clients to jobs, the transaction rules (deposit, withdrawal,
//! dispute, resolve, chargeback), and failed-transaction bookkeeping belong
//! here.

use record_processor_domain::client_id::ClientId;
use record_processor_domain::input::Transaction;
use record_processor_domain::output::AccountBalance;
use storage::{Lookup, Position, ReadAt, Span, Update};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::engine::Engine;

/// The error of reading a batch from the queue `Q`.
pub type ReadFailure<Q> = <Q as ReadAt<Transaction>>::Error;

/// The error of looking up a client's state in `K`.
pub type LookupFailure<K> = <K as Lookup<ClientId, AccountBalance>>::Error;

/// The error of updating a client's state in `K`.
pub type UpdateFailure<K> = <K as Update<ClientId, AccountBalance>>::Error;

/// Failure to aggregate the queue into per-client state.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum AggregateError<R, S, U> {
    /// Aggregation was cancelled before the queue was drained.
    #[error("aggregation cancelled")]
    Cancelled,
    /// Reading a batch from the queue failed.
    #[error("reading a batch from the queue failed: {0}")]
    Queue(R),
    /// Looking up a client's state failed.
    #[error("looking up client state failed: {0}")]
    State(S),
    /// The stage has no business-logic implementation yet.
    #[error("aggregate stage is not implemented")]
    Unimplemented,
    /// Writing a client's state failed.
    #[error("updating client state failed: {0}")]
    Update(U),
}

impl<Q, K> Engine<'_, Q, K>
where
    Q: ReadAt<Transaction>,
    K: Lookup<ClientId, AccountBalance> + Update<ClientId, AccountBalance>,
{
    /// Read the queue from `from` up to the end of `ingested`, one `batch` at
    /// a time, and apply every transaction to per-client state through worker
    /// jobs spawned on `tracker`. Closes `tracker` and waits for every job
    /// before returning, so on success every record of `ingested` from `from`
    /// onward has been applied.
    ///
    /// # Errors
    ///
    /// Returns [`AggregateError::Cancelled`] when the token fires,
    /// [`AggregateError::Queue`], [`AggregateError::State`] or
    /// [`AggregateError::Update`] when a store fails, and
    /// [`AggregateError::Unimplemented`] (contract stub).
    #[inline]
    pub async fn aggregate(
        &self,
        _from: Position,
        _ingested: Span,
        _batch: &mut [Transaction],
        _tracker: &TaskTracker,
        cancel: &CancellationToken,
    ) -> Result<(), AggregateError<ReadFailure<Q>, LookupFailure<K>, UpdateFailure<K>>> {
        if cancel.is_cancelled() {
            Err(AggregateError::Cancelled)
        } else {
            Err(AggregateError::Unimplemented)
        }
    }
}
