//! The per-client key-value state contract.

use record_processor_domain::client_id::ClientId;
use record_processor_domain::output::AccountBalance;
use tokio_util::sync::CancellationToken;

/// Failure to use the per-client state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum StateError {
    /// The state is a contract stub with no implementation yet.
    #[error("per-client state is not implemented")]
    Unimplemented,
}

/// Failure to pull the next entry from the lazy iterator.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum NextEntryError {
    /// The iteration was cancelled.
    #[error("per-client state iteration cancelled")]
    Cancelled,
    /// The iterator is a contract stub with no implementation yet.
    #[error("per-client state iteration is not implemented")]
    Unimplemented,
}

/// A lazy, owned iterator over every `(client, balance)` entry.
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
    /// [`NextEntryError::Unimplemented`] (contract stub).
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

/// Per-client account state, keyed by client id.
#[mockall::automock(type Error = StateError;)]
pub trait KvState {
    /// The error produced by the state.
    type Error: core::fmt::Debug + core::fmt::Display;

    /// Lazily iterate every stored entry.
    fn entries(&mut self) -> KvEntries;

    /// Record `balance` for `client`.
    async fn update(
        &mut self,
        client: ClientId,
        balance: &AccountBalance,
    ) -> Result<(), Self::Error>;
}
