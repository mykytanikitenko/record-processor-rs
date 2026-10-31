//! Error types reported by the mocked contracts.

/// Failure scripted into a mocked [`crate::kv::Kv::load`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("mocked key-value load failure")]
pub struct MockLoadError;

/// Failure scripted into a mocked [`crate::kv::Kv::store`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("mocked key-value store failure")]
pub struct MockStoreError;

/// Failure scripted into a mocked [`crate::kv::Kv::keys`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("mocked key-value keys failure")]
pub struct MockKeysError;

/// Failure scripted into a mocked [`crate::state::State::current_offset`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("mocked current offset failure")]
pub struct MockCurrentOffsetError;

/// Failure scripted into a mocked [`crate::state::State::commit_offset`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("mocked commit offset failure")]
pub struct MockCommitOffsetError;

/// Failure scripted into a mocked [`crate::state::State::update`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("mocked state update failure")]
pub struct MockStateUpdateError;

/// Failure scripted into a mocked [`crate::state::State::load`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("mocked state load failure")]
pub struct MockStateLoadError;
