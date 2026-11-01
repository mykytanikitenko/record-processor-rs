//! The service-side engine state: per-client account state and the transaction
//! lifecycle, composed over the storage key-value contract.
//!
//! The state is deliberately service-owned rather than reusing the storage
//! [`ClientState`](storage::ClientState): that type wraps the *output*
//! [`AccountBalance`](record_processor_domain::output::AccountBalance) row,
//! whose `total` field is derived from `available + held` and whose shape is
//! dictated by CSV serialization. The engine mutates *input* state —
//! `available`, `held`, `locked`, and the per-transaction dispute lifecycle —
//! which has no place in an output projection. Storage therefore stays free of
//! business logic and only persists the opaque state the engine hands it.

use core::error::Error;

use record_processor_domain::client_id::ClientId;
use record_processor_domain::input::TxId;
use rust_decimal::Decimal;
use storage::Kv;
use tokio_util::sync::CancellationToken;

/// The stored state of one client account.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceState {
    /// The funds available for trading or withdrawal.
    available: Decimal,
    /// The funds held under dispute.
    held: Decimal,
    /// Whether the account is frozen after a chargeback.
    locked: bool,
}

impl ServiceState {
    /// The funds available for trading or withdrawal.
    #[must_use]
    #[inline]
    pub const fn available(&self) -> Decimal {
        self.available
    }

    /// The funds held under dispute.
    #[must_use]
    #[inline]
    pub const fn held(&self) -> Decimal {
        self.held
    }

    /// Whether the account is frozen after a chargeback.
    #[must_use]
    #[inline]
    pub const fn locked(&self) -> bool {
        self.locked
    }
}

/// Input fields for constructing a [`ServiceState`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceStateParts {
    /// The funds available for trading or withdrawal.
    pub available: Decimal,
    /// The funds held under dispute.
    pub held: Decimal,
    /// Whether the account is frozen after a chargeback.
    pub locked: bool,
}

impl From<ServiceStateParts> for ServiceState {
    #[inline]
    fn from(parts: ServiceStateParts) -> Self {
        Self {
            available: parts.available,
            held: parts.held,
            locked: parts.locked,
        }
    }
}

/// Whether a recorded transaction credited or debited its client.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TxKind {
    /// The transaction credited the client.
    Deposit,
    /// The transaction debited the client.
    Withdrawal,
}

/// The lifecycle stage of a recorded transaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TxStatus {
    /// The transaction is intact and its funds are settled.
    Available,
    /// The deposit was reversed by a chargeback and its funds are gone.
    ChargedBack,
    /// The deposit is under dispute and its funds are held.
    Disputed,
}

/// The stored lifecycle of one transaction, referenced by later dispute,
/// resolve, or chargeback actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TxRecord {
    /// The transaction amount.
    amount: Decimal,
    /// The client the transaction was applied to.
    client: ClientId,
    /// Whether the transaction is a deposit or a withdrawal.
    kind: TxKind,
    /// The current lifecycle stage.
    status: TxStatus,
}

impl TxRecord {
    /// The transaction amount.
    #[must_use]
    #[inline]
    pub const fn amount(&self) -> Decimal {
        self.amount
    }

    /// The client the transaction was applied to.
    #[must_use]
    #[inline]
    pub const fn client(&self) -> ClientId {
        self.client
    }

    /// Whether the transaction is a deposit or a withdrawal.
    #[must_use]
    #[inline]
    pub const fn kind(&self) -> TxKind {
        self.kind
    }

    /// The current lifecycle stage.
    #[must_use]
    #[inline]
    pub const fn status(&self) -> TxStatus {
        self.status
    }
}

/// Input fields for constructing a [`TxRecord`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TxRecordParts {
    /// The transaction amount.
    pub amount: Decimal,
    /// The client the transaction was applied to.
    pub client: ClientId,
    /// Whether the transaction is a deposit or a withdrawal.
    pub kind: TxKind,
    /// The current lifecycle stage.
    pub status: TxStatus,
}

impl From<TxRecordParts> for TxRecord {
    #[inline]
    fn from(parts: TxRecordParts) -> Self {
        Self {
            amount: parts.amount,
            client: parts.client,
            kind: parts.kind,
            status: parts.status,
        }
    }
}

/// The engine state: per-client account state and the transaction lifecycle.
pub trait State {
    /// The error produced by [`State::next_client`].
    type ClientsError: Error;
    /// The error produced by [`State::load_client`].
    type LoadClientError: Error;
    /// The error produced by [`State::load_tx`].
    type LoadTxError: Error;
    /// The error produced by [`State::store_client`].
    type StoreClientError: Error;
    /// The error produced by [`State::store_tx`].
    type StoreTxError: Error;

    /// Return the account state stored for `client`, or `None` when absent.
    fn load_client(
        &self,
        client: ClientId,
        cancel: &CancellationToken,
    ) -> impl Future<Output = Result<Option<ServiceState>, Self::LoadClientError>>;

    /// Return the transaction lifecycle stored for `tx`, or `None` when absent.
    fn load_tx(
        &self,
        tx: TxId,
        cancel: &CancellationToken,
    ) -> impl Future<Output = Result<Option<TxRecord>, Self::LoadTxError>>;

    /// Return the smallest stored client strictly greater than `after`, in
    /// ascending client order, or `None` when `after` is the greatest client.
    /// Pass `None` to obtain the first client.
    fn next_client(
        &self,
        after: Option<ClientId>,
        cancel: &CancellationToken,
    ) -> impl Future<Output = Result<Option<(ClientId, ServiceState)>, Self::ClientsError>>;

    /// Store `state` for `client`, replacing any previous account state.
    fn store_client(
        &self,
        client: ClientId,
        state: ServiceState,
        cancel: &CancellationToken,
    ) -> impl Future<Output = Result<(), Self::StoreClientError>>;

    /// Store `state` for `tx`, replacing any previous transaction lifecycle.
    fn store_tx(
        &self,
        tx: TxId,
        state: TxRecord,
        cancel: &CancellationToken,
    ) -> impl Future<Output = Result<(), Self::StoreTxError>>;
}

#[cfg(feature = "mocks")]
mockall::mock! {
    #[derive(Debug)]
    pub State {
    }
    impl State for State {
        type ClientsError = crate::mocks::MockClientsError;
        type LoadClientError = crate::mocks::MockLoadClientError;
        type LoadTxError = crate::mocks::MockLoadTxError;
        type StoreClientError = crate::mocks::MockStoreClientError;
        type StoreTxError = crate::mocks::MockStoreTxError;

        async fn load_client(&self, client: ClientId, cancel: &CancellationToken) -> Result<Option<ServiceState>, crate::mocks::MockLoadClientError>;

        async fn load_tx(&self, tx: TxId, cancel: &CancellationToken) -> Result<Option<TxRecord>, crate::mocks::MockLoadTxError>;

        async fn next_client(&self, after: Option<ClientId>, cancel: &CancellationToken) -> Result<Option<(ClientId, ServiceState)>, crate::mocks::MockClientsError>;

        async fn store_client(&self, client: ClientId, state: ServiceState, cancel: &CancellationToken) -> Result<(), crate::mocks::MockStoreClientError>;

        async fn store_tx(&self, tx: TxId, state: TxRecord, cancel: &CancellationToken) -> Result<(), crate::mocks::MockStoreTxError>;
    }
}

/// The [`State`] composed over a client store and a transaction store.
#[derive(Clone, Copy, Debug)]
pub struct KvState<'kv, C, D> {
    /// The store holding each client's account state.
    pub clients: &'kv C,
    /// The store holding each transaction's lifecycle.
    pub transactions: &'kv D,
}

/// Failure to load a client's account state.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoadClientError<E> {
    /// The client store failed.
    #[error("loading the client state failed: {0}")]
    Load(E),
}

/// Failure to load a transaction's lifecycle.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoadTxError<E> {
    /// The transaction store failed.
    #[error("loading the transaction state failed: {0}")]
    Load(E),
}

/// Failure to store a client's account state.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreClientError<E> {
    /// The client store failed.
    #[error("storing the client state failed: {0}")]
    Store(E),
}

/// Failure to store a transaction's lifecycle.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreTxError<E> {
    /// The transaction store failed.
    #[error("storing the transaction state failed: {0}")]
    Store(E),
}

/// Failure to enumerate the stored clients.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ClientsError<K, L> {
    /// The client store failed to enumerate its keys.
    #[error("enumerating client keys failed: {0}")]
    Keys(K),
    /// The client store failed to load a client's state.
    #[error("loading a client's state failed: {0}")]
    Load(L),
}

impl<C, D> State for KvState<'_, C, D>
where
    C: Kv<ClientId, ServiceState>,
    D: Kv<TxId, TxRecord>,
{
    type ClientsError = ClientsError<C::KeysError, C::LoadError>;
    type LoadClientError = LoadClientError<C::LoadError>;
    type LoadTxError = LoadTxError<D::LoadError>;
    type StoreClientError = StoreClientError<C::StoreError>;
    type StoreTxError = StoreTxError<D::StoreError>;

    #[inline]
    async fn load_client(
        &self,
        client: ClientId,
        cancel: &CancellationToken,
    ) -> Result<Option<ServiceState>, Self::LoadClientError> {
        self.clients
            .load(client, cancel)
            .await
            .map_err(LoadClientError::Load)
    }

    #[inline]
    async fn load_tx(
        &self,
        tx: TxId,
        cancel: &CancellationToken,
    ) -> Result<Option<TxRecord>, Self::LoadTxError> {
        self.transactions
            .load(tx, cancel)
            .await
            .map_err(LoadTxError::Load)
    }

    #[inline]
    async fn next_client(
        &self,
        after: Option<ClientId>,
        cancel: &CancellationToken,
    ) -> Result<Option<(ClientId, ServiceState)>, Self::ClientsError> {
        let mut cursor = after;
        loop {
            let Some(client) = self
                .clients
                .next_key(cursor, cancel)
                .await
                .map_err(ClientsError::Keys)?
            else {
                return Ok(None);
            };
            if let Some(state) = self
                .clients
                .load(client, cancel)
                .await
                .map_err(ClientsError::Load)?
            {
                return Ok(Some((client, state)));
            }
            cursor = Some(client);
        }
    }

    #[inline]
    async fn store_client(
        &self,
        client: ClientId,
        state: ServiceState,
        cancel: &CancellationToken,
    ) -> Result<(), Self::StoreClientError> {
        self.clients
            .store(client, state, cancel)
            .await
            .map_err(StoreClientError::Store)
    }

    #[inline]
    async fn store_tx(
        &self,
        tx: TxId,
        state: TxRecord,
        cancel: &CancellationToken,
    ) -> Result<(), Self::StoreTxError> {
        self.transactions
            .store(tx, state, cancel)
            .await
            .map_err(StoreTxError::Store)
    }
}
