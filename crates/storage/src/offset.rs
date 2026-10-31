//! The position of a record in a queue.

/// The zero-based position of a record since the start of a queue.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, derive_more::Deref, derive_more::From,
)]
pub struct Offset(u64);
