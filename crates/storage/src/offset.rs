//! The `Offset` record position in the append-only queue.

/// A record position in the queue: the zero-based index of a record since the
/// start of the queue. Read through [`Deref`](core::ops::Deref) as a `u64`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, derive_more::Deref)]
pub struct Offset(u64);

impl From<u64> for Offset {
    #[inline]
    fn from(index: u64) -> Self {
        Self(index)
    }
}
