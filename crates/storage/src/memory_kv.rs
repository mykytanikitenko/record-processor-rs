//! The in-memory key-value backing.

use core::hash::{BuildHasher, Hash};

use allocator_api2::alloc::Allocator;
use hashbrown::HashMap;
use tokio_util::sync::CancellationToken;

use crate::kv::Kv;

/// Failure to enumerate the keys of the in-memory key-value store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum KeysError {
    /// The key enumeration was cancelled before the table was read.
    #[error("key enumeration cancelled")]
    Cancelled,
}

/// Failure to load a value from the in-memory key-value store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum LoadError {
    /// The load was cancelled before the table was read.
    #[error("load cancelled")]
    Cancelled,
}

/// The in-memory key-value backing, allocating in the caller's `A` allocator.
#[derive(Debug)]
pub struct MemoryKv<K, V, S, A>
where
    A: Allocator,
{
    /// The table, locked for the duration of each operation.
    table: async_lock::Mutex<HashMap<K, V, S, A>>,
}

impl<K, V, S, A> MemoryKv<K, V, S, A>
where
    K: Eq + Hash + Clone,
    V: Copy,
    S: BuildHasher,
    A: Allocator,
{
    /// Read the value for `key` under the table lock.
    async fn read(&self, key: K) -> Option<V> {
        self.table.lock().await.get(&key).copied()
    }

    /// Write `value` for `key` under the table lock.
    async fn write(&self, key: K, value: V) -> Result<(), StoreError> {
        let mut table = self.table.lock().await;
        table.try_reserve(1)?;
        let _previous: Option<V> = table.insert(key, value);
        drop(table);
        Ok(())
    }
}

impl<K, V, S, A> MemoryKv<K, V, S, A>
where
    K: Eq + Hash + Clone + Ord,
    V: Copy,
    S: BuildHasher,
    A: Allocator,
{
    /// Return the smallest stored key strictly greater than `after`, if any.
    async fn next_key_inner(&self, after: Option<K>) -> Option<K> {
        let table = self.table.lock().await;
        let mut next: Option<K> = None;
        for key in table.keys() {
            let greater = after.as_ref().is_none_or(|after_key| key > after_key);
            let smaller = next.as_ref().is_none_or(|candidate| key < candidate);
            if greater && smaller {
                next = Some(key.clone());
            }
        }
        drop(table);
        next
    }
}

impl<K, V, S, A> From<Parts<S, A>> for MemoryKv<K, V, S, A>
where
    A: Allocator,
{
    #[inline]
    fn from(parts: Parts<S, A>) -> Self {
        Self {
            table: async_lock::Mutex::new(HashMap::with_hasher_in(parts.hasher, parts.alloc)),
        }
    }
}

impl<K, V, S, A> Kv<K, V> for MemoryKv<K, V, S, A>
where
    K: Eq + Hash + Clone + Ord,
    V: Copy,
    S: BuildHasher,
    A: Allocator,
{
    type KeysError = KeysError;
    type LoadError = LoadError;
    type StoreError = StoreError;

    #[inline]
    async fn load(&self, key: K, cancel: &CancellationToken) -> Result<Option<V>, Self::LoadError> {
        cancel
            .run_until_cancelled(self.read(key))
            .await
            .ok_or(LoadError::Cancelled)
    }

    #[inline]
    async fn next_key(
        &self,
        after: Option<K>,
        cancel: &CancellationToken,
    ) -> Result<Option<K>, Self::KeysError> {
        cancel
            .run_until_cancelled(self.next_key_inner(after))
            .await
            .ok_or(KeysError::Cancelled)
    }

    #[inline]
    async fn store(
        &self,
        key: K,
        value: V,
        cancel: &CancellationToken,
    ) -> Result<(), Self::StoreError> {
        cancel
            .run_until_cancelled(self.write(key, value))
            .await
            .ok_or(StoreError::Cancelled)?
    }
}

/// Construction input for [`MemoryKv`].
#[derive(Clone, Copy, Debug)]
pub struct Parts<S, A> {
    /// The allocator the table is allocated in.
    pub alloc: A,
    /// The hasher builder keys are hashed with.
    pub hasher: S,
}

/// Failure to store a value in the in-memory key-value store.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// The table could not grow to hold the new key.
    #[error("key-value allocation failed: {0}")]
    Alloc(#[from] hashbrown::TryReserveError),
    /// The store was cancelled before the table was updated.
    #[error("store cancelled")]
    Cancelled,
}
