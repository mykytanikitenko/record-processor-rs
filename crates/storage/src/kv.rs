//! The key-value state contract: load and store a value per key.

// TODO(lint/E0433): cannot find module or crate `tokio_util` in this scope
use tokio_util::sync::CancellationToken;

/// A key-value store of `V` values keyed by `K`.
// TODO(lint/unexpected_cfgs): unexpected `cfg` condition value: `mocks`
#[cfg_attr(feature = "mocks", automock)]
pub trait Kv<K, V> {
    /// The error produced by [`Kv::load`] and [`Kv::store`].
    type Error: core::error::Error;

    /// Return the value stored for `key`, or `None` when absent.
    async fn load(&self, key: K, cancel: &CancellationToken) -> Result<Option<V>, Self::Error>;

    /// Store `value` for `key`, replacing any previous value.
    async fn store(&self, key: K, value: V, cancel: &CancellationToken) -> Result<(), Self::Error>;
}
