//! Client identifier.

/// A client identifier, shared by the input and output models.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, derive_more::Deref, serde::Serialize,
)]
pub struct ClientId(u16);

impl From<u16> for ClientId {
    #[inline]
    fn from(value: u16) -> Self {
        Self(value)
    }
}
