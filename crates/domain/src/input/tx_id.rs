//! Transaction identifier.

/// A globally unique transaction identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, derive_more::Deref)]
pub struct TxId(u32);

impl From<u32> for TxId {
    #[inline]
    fn from(value: u32) -> Self {
        Self(value)
    }
}
