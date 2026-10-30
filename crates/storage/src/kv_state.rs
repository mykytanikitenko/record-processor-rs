//! The per-client key-value state contract.

use record_processor_domain::client_id::ClientId;
use record_processor_domain::output::AccountBalance;
use tokio_util::sync::CancellationToken;


// Storage:
//     Define Offset u64 type
//     Queue: - Generic queue
//         async append batch, returns Offset
//         async subscribe(since: Offset) -> Lazy stream of batches
//     implementation enum: Memory (backed by regular vector), might be real db in future
//
//     KV: Represents generic KV storage
//        async get
//        async set
//     implementation: Memory (backed by regular hashmap), might be real db in future
//
//

/// Failure to read one client's state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum StateReadError {
    /// The read was cancelled.
    #[error("per-client state read cancelled")]
    Cancelled,
    /// The state is a contract stub with no backing store yet.
    #[error("per-client state read is not implemented")]
    Unimplemented,
}

/// Failure to write one client's state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum StateWriteError {
    /// The write was cancelled before it was persisted.
    #[error("per-client state write cancelled")]
    Cancelled,
    /// The state is a contract stub with no backing store yet.
    #[error("per-client state write is not implemented")]
    Unimplemented,
}

/// Failure to pull the next entry from the lazy iterator.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum NextEntryError {
    /// The iteration was cancelled.
    #[error("per-client state iteration cancelled")]
    Cancelled,
    /// The iterator is a contract stub with no backing store yet.
    #[error("per-client state iteration is not implemented")]
    Unimplemented,
}

/// A lazy, owned cursor over every stored `(client, balance)` entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KvEntries {
    /// Whether the single stub error item has already been yielded.
    yielded: bool,
}

impl From<()> for KvEntries {
    #[inline]
    fn from((): ()) -> Self {
        Self { yielded: false }
    }
}

impl KvEntries {
    /// Pull the next entry, or `None` once the iteration is exhausted.
    ///
    /// # Errors
    ///
    /// Returns [`NextEntryError::Cancelled`] when the token fires, or
    /// [`NextEntryError::Unimplemented`] while there is no backing store.
    #[inline]
    pub async fn next(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<(ClientId, AccountBalance)>, NextEntryError> {
        if cancel.is_cancelled() {
            Err(NextEntryError::Cancelled)
        } else if self.yielded {
            Ok(None)
        } else {
            self.yielded = true;
            Err(NextEntryError::Unimplemented)
        }
    }
}

/// Per-client account state keyed by client id; stores whatever the service
/// hands it and applies no rules of its own.
pub trait KvState {
    /// The error produced by [`KvState::read`].
    type ReadStateError: core::fmt::Debug + core::fmt::Display;
    /// The error produced by [`KvState::write`].
    type WriteStateError: core::fmt::Debug + core::fmt::Display;

    /// Lazily iterate every stored entry.
    fn entries(&mut self) -> KvEntries;

    /// Load the stored balance for `client`, or `None` if never written.
    async fn read(
        &mut self,
        client: ClientId,
        cancel: &CancellationToken,
    ) -> Result<Option<AccountBalance>, Self::ReadStateError>;

    /// Persist `balance` for `client`, replacing any previous value.
    async fn write(
        &mut self,
        client: ClientId,
        balance: &AccountBalance,
        cancel: &CancellationToken,
    ) -> Result<(), Self::WriteStateError>;
}
