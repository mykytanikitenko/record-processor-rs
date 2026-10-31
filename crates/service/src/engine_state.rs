//! The service-side engine state: per-client account state and the deposit
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

/// The lifecycle stage of a recorded deposit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DepositStatus {
    /// The deposit is intact and its funds are available.
    Available,
    /// The deposit was reversed by a chargeback and its funds are gone.
    ChargedBack,
    /// The deposit is under dispute and its funds are held.
    Disputed,
}

/// The stored lifecycle of one deposit, referenced by later dispute, resolve,
/// or chargeback actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DepositState {
    /// The deposited amount.
    amount: Decimal,
    /// The client the deposit was credited to.
    client: ClientId,
    /// The current lifecycle stage.
    status: DepositStatus,
}

impl DepositState {
    /// The deposited amount.
    #[must_use]
    #[inline]
    pub const fn amount(&self) -> Decimal {
        self.amount
    }

    /// The client the deposit was credited to.
    #[must_use]
    #[inline]
    pub const fn client(&self) -> ClientId {
        self.client
    }

    /// The current lifecycle stage.
    #[must_use]
    #[inline]
    pub const fn status(&self) -> DepositStatus {
        self.status
    }
}

/// Input fields for constructing a [`DepositState`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DepositStateParts {
    /// The deposited amount.
    pub amount: Decimal,
    /// The client the deposit was credited to.
    pub client: ClientId,
    /// The current lifecycle stage.
    pub status: DepositStatus,
}

impl From<DepositStateParts> for DepositState {
    #[inline]
    fn from(parts: DepositStateParts) -> Self {
        Self {
            amount: parts.amount,
            client: parts.client,
            status: parts.status,
        }
    }
}

/// The engine state: per-client account state and the deposit lifecycle.
#[cfg_attr(
    feature = "mocks",
    mockall::automock(
        type LoadClientError = crate::mocks::MockLoadClientError;
        type LoadDepositError = crate::mocks::MockLoadDepositError;
        type StoreClientError = crate::mocks::MockStoreClientError;
        type StoreDepositError = crate::mocks::MockStoreDepositError;
        type ClientsError = crate::mocks::MockClientsError;
    )
)]
pub trait State {
    /// The error produced by [`State::next_client`].
    type ClientsError: Error;
    /// The error produced by [`State::load_client`].
    type LoadClientError: Error;
    /// The error produced by [`State::load_deposit`].
    type LoadDepositError: Error;
    /// The error produced by [`State::store_client`].
    type StoreClientError: Error;
    /// The error produced by [`State::store_deposit`].
    type StoreDepositError: Error;

    /// Return the account state stored for `client`, or `None` when absent.
    async fn load_client(
        &self,
        client: ClientId,
        cancel: &CancellationToken,
    ) -> Result<Option<ServiceState>, Self::LoadClientError>;

    /// Return the deposit lifecycle stored for `tx`, or `None` when absent.
    async fn load_deposit(
        &self,
        tx: TxId,
        cancel: &CancellationToken,
    ) -> Result<Option<DepositState>, Self::LoadDepositError>;

    /// Return the smallest stored client strictly greater than `after`, in
    /// ascending client order, or `None` when `after` is the greatest client.
    /// Pass `None` to obtain the first client.
    async fn next_client(
        &self,
        after: Option<ClientId>,
        cancel: &CancellationToken,
    ) -> Result<Option<(ClientId, ServiceState)>, Self::ClientsError>;

    /// Store `state` for `client`, replacing any previous account state.
    async fn store_client(
        &self,
        client: ClientId,
        state: ServiceState,
        cancel: &CancellationToken,
    ) -> Result<(), Self::StoreClientError>;

    /// Store `state` for `tx`, replacing any previous deposit lifecycle.
    async fn store_deposit(
        &self,
        tx: TxId,
        state: DepositState,
        cancel: &CancellationToken,
    ) -> Result<(), Self::StoreDepositError>;
}

/// The [`State`] composed over a client store and a deposit store.
#[derive(Clone, Copy, Debug)]
pub struct KvState<'kv, C, D> {
    /// The store holding each client's account state.
    pub clients: &'kv C,
    /// The store holding each deposit's lifecycle.
    pub deposits: &'kv D,
}

/// Failure to load a client's account state.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoadClientError<E> {
    /// The client store failed.
    #[error("loading the client state failed: {0}")]
    Load(E),
}

/// Failure to load a deposit's lifecycle.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoadDepositError<E> {
    /// The deposit store failed.
    #[error("loading the deposit state failed: {0}")]
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

/// Failure to store a deposit's lifecycle.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreDepositError<E> {
    /// The deposit store failed.
    #[error("storing the deposit state failed: {0}")]
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
    D: Kv<TxId, DepositState>,
{
    type ClientsError = ClientsError<C::KeysError, C::LoadError>;
    type LoadClientError = LoadClientError<C::LoadError>;
    type LoadDepositError = LoadDepositError<D::LoadError>;
    type StoreClientError = StoreClientError<C::StoreError>;
    type StoreDepositError = StoreDepositError<D::StoreError>;

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
    async fn load_deposit(
        &self,
        tx: TxId,
        cancel: &CancellationToken,
    ) -> Result<Option<DepositState>, Self::LoadDepositError> {
        self.deposits
            .load(tx, cancel)
            .await
            .map_err(LoadDepositError::Load)
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
    async fn store_deposit(
        &self,
        tx: TxId,
        state: DepositState,
        cancel: &CancellationToken,
    ) -> Result<(), Self::StoreDepositError> {
        self.deposits
            .store(tx, state, cancel)
            .await
            .map_err(StoreDepositError::Store)
    }
}
