//! The memory-mapped key-value backing (a contract stub until the backing
//! lands).

// TODO(lint/E0432): unresolved imports `crate::kv::Entry`, `crate::kv::Lookup`, `crate::kv::Slot`, `crate::kv::Update`, `crate::kv::Walk`
use crate::kv::{Entry, Lookup, Slot, Update, Walk};

/// Failure to look up a value in the memory-mapped key-value store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum LookupError {
    /// The backing has no implementation yet.
    #[error("memory-mapped key-value lookup is not implemented")]
    Unimplemented,
}

/// Failure to update the memory-mapped key-value store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum UpdateError {
    /// The backing has no implementation yet.
    #[error("memory-mapped key-value update is not implemented")]
    Unimplemented,
}

/// Failure to walk the memory-mapped key-value store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum WalkError {
    /// The backing has no implementation yet.
    #[error("memory-mapped key-value walk is not implemented")]
    Unimplemented,
}

/// The memory-mapped key-value backing. Every method returns its typed
/// `Unimplemented` error.
#[derive(Clone, Copy, Debug)]
pub struct MmapKv;

impl<K, V> Lookup<K, V> for MmapKv
where
    K: Copy,
    V: Copy,
{
    type Error = LookupError;

    #[inline]
    async fn read(&self, _key: K) -> Result<Option<V>, Self::Error> {
        Err(LookupError::Unimplemented)
    }
}

impl<K, V> Update<K, V> for MmapKv
where
    K: Copy,
    V: Copy,
{
    type Error = UpdateError;

    #[inline]
    async fn update(&self, _key: K, _value: &V) -> Result<(), Self::Error> {
        Err(UpdateError::Unimplemented)
    }
}

impl<K, V> Walk<K, V> for MmapKv
where
    K: Copy,
    V: Copy,
{
    type Error = WalkError;

    #[inline]
    async fn next_entry(&self, _from: Slot) -> Result<Option<Entry<K, V>>, Self::Error> {
        Err(WalkError::Unimplemented)
    }
}
