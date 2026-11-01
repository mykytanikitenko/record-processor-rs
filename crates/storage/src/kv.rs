//! The key-value contract: load and store a value per key.

use core::error::Error;

use tokio_util::sync::CancellationToken;

/// A key-value store of `V` values keyed by `K`.
///
/// Takes `&self` so workers share the store by reference; the backing uses
/// interior mutability, relying on the single-threaded runtime for safety.
pub trait Kv<K, V> {
    /// The error produced by [`Kv::next_key`].
    type KeysError: Error;
    /// The error produced by [`Kv::load`].
    type LoadError: Error;
    /// The error produced by [`Kv::store`].
    type StoreError: Error;

    /// Return the value stored for `key`, or `None` when absent.
    fn load(
        &self,
        key: K,
        cancel: &CancellationToken,
    ) -> impl Future<Output = Result<Option<V>, Self::LoadError>>;

    /// Return the smallest key strictly greater than `after`, or `None` when
    /// `after` is the greatest key. Pass `None` to obtain the smallest key.
    fn next_key(
        &self,
        after: Option<K>,
        cancel: &CancellationToken,
    ) -> impl Future<Output = Result<Option<K>, Self::KeysError>>;

    /// Store `value` for `key`, replacing any previous value.
    fn store(
        &self,
        key: K,
        value: V,
        cancel: &CancellationToken,
    ) -> impl Future<Output = Result<(), Self::StoreError>>;
}

#[cfg(feature = "mocks")]
mockall::mock! {
    #[derive(Debug)]
    pub Kv<K, V> {
    }
    impl<K, V> Kv<K, V> for Kv<K, V> {
        type KeysError = crate::mocks::MockKeysError;
        type LoadError = crate::mocks::MockLoadError;
        type StoreError = crate::mocks::MockStoreError;

        async fn load(&self, key: K, cancel: &CancellationToken) -> Result<Option<V>, crate::mocks::MockLoadError>;

        async fn next_key(&self, after: Option<K>, cancel: &CancellationToken) -> Result<Option<K>, crate::mocks::MockKeysError>;

        async fn store(&self, key: K, value: V, cancel: &CancellationToken) -> Result<(), crate::mocks::MockStoreError>;
    }
}
