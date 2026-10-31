//! The key-value contract: load and store a value per key.

use core::error::Error;

use tokio_util::sync::CancellationToken;

/// A key-value store of `V` values keyed by `K`.
///
/// Takes `&self` so workers share the store by reference; a backing
/// synchronises access internally.
#[cfg_attr(
    feature = "mocks",
    mockall::automock(
        type KeysError = crate::mocks::MockKeysError;
        type LoadError = crate::mocks::MockLoadError;
        type StoreError = crate::mocks::MockStoreError;
    )
)]
pub trait Kv<K, V> {
    /// The error produced by [`Kv::keys`].
    type KeysError: Error;
    /// The error produced by [`Kv::load`].
    type LoadError: Error;
    /// The error produced by [`Kv::store`].
    type StoreError: Error;

    /// Return every key currently stored.
    async fn keys(&self, cancel: &CancellationToken) -> Result<Vec<K>, Self::KeysError>;

    /// Return the value stored for `key`, or `None` when absent.
    async fn load(&self, key: K, cancel: &CancellationToken) -> Result<Option<V>, Self::LoadError>;

    /// Store `value` for `key`, replacing any previous value.
    async fn store(
        &self,
        key: K,
        value: V,
        cancel: &CancellationToken,
    ) -> Result<(), Self::StoreError>;
}
