//! The engine state contract: the committed queue offset and per-client
//! account state, composed over key-value stores.

use core::error::Error;

use record_processor_domain::client_id::ClientId;
use record_processor_domain::output::AccountBalance;
use tokio_util::sync::CancellationToken;

use crate::kv::Kv;
use crate::offset::Offset;

/// The stored state of one client account.
#[derive(Clone, Copy, Debug, Eq, PartialEq, derive_more::Deref, derive_more::From)]
pub struct ClientState(AccountBalance);

/// The key the committed offset is stored under.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CommittedOffset;

/// The engine state: the queue offset processed so far and every client's
/// account state.
#[cfg_attr(
    feature = "mocks",
    mockall::automock(
        type CurrentOffsetError = crate::mocks::MockCurrentOffsetError;
        type CommitOffsetError = crate::mocks::MockCommitOffsetError;
        type StateUpdateError = crate::mocks::MockStateUpdateError;
        type StateLoadError = crate::mocks::MockStateLoadError;
    )
)]
pub trait State {
    /// The error produced by [`State::current_offset`].
    type CurrentOffsetError: Error;
    /// The error produced by [`State::commit_offset`].
    type CommitOffsetError: Error;
    /// The error produced by [`State::update`].
    type StateUpdateError: Error;
    /// The error produced by [`State::load`].
    type StateLoadError: Error;

    /// Return the committed offset, or `None` before the first commit.
    async fn current_offset(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Option<Offset>, Self::CurrentOffsetError>;

    /// Record `offset` as the offset processed so far.
    async fn commit_offset(
        &self,
        offset: Offset,
        cancel: &CancellationToken,
    ) -> Result<(), Self::CommitOffsetError>;

    /// Store `state` for `client`, replacing any previous state.
    async fn update(
        &self,
        client: ClientId,
        state: ClientState,
        cancel: &CancellationToken,
    ) -> Result<(), Self::StateUpdateError>;

    /// Return the state stored for `client`, or `None` when absent.
    async fn load(
        &self,
        client: ClientId,
        cancel: &CancellationToken,
    ) -> Result<Option<ClientState>, Self::StateLoadError>;
}

/// The [`State`] composed over a committed-offset store and a client store.
#[derive(Clone, Copy, Debug)]
pub struct KvState<'kv, O, C> {
    /// The store holding the committed offset.
    pub offsets: &'kv O,
    /// The store holding each client's state.
    pub clients: &'kv C,
}

/// Failure to read the committed offset.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CurrentOffsetError<E> {
    /// The offset store failed.
    #[error("loading the committed offset failed: {0}")]
    Load(E),
}

/// Failure to commit an offset.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CommitOffsetError<E> {
    /// The offset store failed.
    #[error("committing the offset failed: {0}")]
    Store(E),
}

/// Failure to update a client's state.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StateUpdateError<E> {
    /// The client store failed.
    #[error("storing the client state failed: {0}")]
    Store(E),
}

/// Failure to load a client's state.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StateLoadError<E> {
    /// The client store failed.
    #[error("loading the client state failed: {0}")]
    Load(E),
}

impl<O, C> State for KvState<'_, O, C>
where
    O: Kv<CommittedOffset, Offset>,
    C: Kv<ClientId, ClientState>,
{
    type CurrentOffsetError = CurrentOffsetError<O::LoadError>;
    type CommitOffsetError = CommitOffsetError<O::StoreError>;
    type StateUpdateError = StateUpdateError<C::StoreError>;
    type StateLoadError = StateLoadError<C::LoadError>;

    #[inline]
    async fn current_offset(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Option<Offset>, Self::CurrentOffsetError> {
        self.offsets
            .load(CommittedOffset, cancel)
            .await
            .map_err(CurrentOffsetError::Load)
    }

    #[inline]
    async fn commit_offset(
        &self,
        offset: Offset,
        cancel: &CancellationToken,
    ) -> Result<(), Self::CommitOffsetError> {
        self.offsets
            .store(CommittedOffset, offset, cancel)
            .await
            .map_err(CommitOffsetError::Store)
    }

    #[inline]
    async fn update(
        &self,
        client: ClientId,
        state: ClientState,
        cancel: &CancellationToken,
    ) -> Result<(), Self::StateUpdateError> {
        self.clients
            .store(client, state, cancel)
            .await
            .map_err(StateUpdateError::Store)
    }

    #[inline]
    async fn load(
        &self,
        client: ClientId,
        cancel: &CancellationToken,
    ) -> Result<Option<ClientState>, Self::StateLoadError> {
        self.clients
            .load(client, cancel)
            .await
            .map_err(StateLoadError::Load)
    }
}
