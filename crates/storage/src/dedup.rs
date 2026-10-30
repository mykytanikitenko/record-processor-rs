//! The transaction-id deduplication index contract.

use record_processor_domain::input::TxId;

/// Failure to use the deduplication index.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum IndexError {
    /// The transaction id was already inserted.
    #[error("duplicate transaction id {0:?}")]
    Duplicate(TxId),
    /// The index is a contract stub with no implementation yet.
    #[error("deduplication index is not implemented")]
    Unimplemented,
}

/// An index of seen transaction ids, rejecting duplicates.
pub trait Index {
    /// The error produced by the index.
    type Error: core::fmt::Debug + core::fmt::Display;

    /// Record `tx` as seen, failing with [`IndexError::Duplicate`] if it was
    /// already present.
    async fn insert(&mut self, tx: &TxId) -> Result<(), Self::Error>;
}
