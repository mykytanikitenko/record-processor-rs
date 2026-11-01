//! Error types reported by the mocked service contracts.

/// Failure scripted into a mocked [`crate::engine_state::State::load_client`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("mocked client state load failure")]
pub struct MockLoadClientError;

/// Failure scripted into a mocked [`crate::engine_state::State::store_client`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("mocked client state store failure")]
pub struct MockStoreClientError;

/// Failure scripted into a mocked [`crate::engine_state::State::load_tx`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("mocked transaction state load failure")]
pub struct MockLoadTxError;

/// Failure scripted into a mocked [`crate::engine_state::State::store_tx`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("mocked transaction state store failure")]
pub struct MockStoreTxError;

/// Failure scripted into a mocked [`crate::engine_state::State::next_client`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("mocked client enumeration failure")]
pub struct MockClientsError;
