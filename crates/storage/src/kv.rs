//! The key-value state contract: look up and update a value per key, and walk
//! every stored entry lazily.

/// An opaque iteration cursor over the stored entries. [`Slot::START`] begins
/// a walk; each [`Entry`] carries the cursor of its successor.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Slot(u64);

impl Slot {
    /// The cursor of the first entry.
    pub const START: Self = Self(0);
}

impl From<u64> for Slot {
    #[inline]
    fn from(index: u64) -> Self {
        Self(index)
    }
}

/// One stored entry and the cursor of the next one.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Entry<K, V> {
    /// The entry key.
    pub key: K,
    /// The cursor to pass to [`Walk::next_entry`] for the following entry.
    pub next: Slot,
    /// The entry value.
    pub value: V,
}

/// Looking up the fixed-size `V` value stored for a `K` key.
///
/// Takes `&self` so concurrent workers share the store by reference; a backing
/// synchronises per key internally.
pub trait Lookup<K, V>
where
    K: Copy,
    V: Copy,
{
    /// The error produced by [`Lookup::read`].
    type Error: core::error::Error;

    /// Return the value stored for `key`, or `None` when absent.
    async fn read(&self, key: K) -> Result<Option<V>, Self::Error>;
}

/// Storing the fixed-size `V` value for a `K` key.
pub trait Update<K, V>
where
    K: Copy,
    V: Copy,
{
    /// The error produced by [`Update::update`].
    type Error: core::error::Error;

    /// Store `value` for `key`, replacing any previous value.
    async fn update(&self, key: K, value: &V) -> Result<(), Self::Error>;
}

/// Walking every stored entry lazily, one cursor step at a time.
pub trait Walk<K, V>
where
    K: Copy,
    V: Copy,
{
    /// The error produced by [`Walk::next_entry`].
    type Error: core::error::Error;

    /// Return the entry stored at or after `from`, or `None` once every entry
    /// has been visited.
    async fn next_entry(&self, from: Slot) -> Result<Option<Entry<K, V>>, Self::Error>;
}

/// A key-value store of fixed-size `V` values keyed by `K`: every type that can
/// [`Lookup`], [`Update`], and [`Walk`].
pub trait Kv<K, V>: Lookup<K, V> + Update<K, V> + Walk<K, V>
where
    K: Copy,
    V: Copy,
{
}

impl<K, V, S> Kv<K, V> for S
where
    K: Copy,
    V: Copy,
    S: Lookup<K, V> + Update<K, V> + Walk<K, V>,
{
}
