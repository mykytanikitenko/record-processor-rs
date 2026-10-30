//! The transaction-id deduplication index contract.

use record_processor_domain::input::TxId;
use tokio_util::sync::CancellationToken;

/// Whether an inserted id was new; the index reports, the service decides.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Insertion {
    /// The id had already been inserted.
    Duplicate,
    /// The id was recorded for the first time.
    Fresh,
}

/// Failure to insert into the deduplication index.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum IndexInsertError {
    /// The insert was cancelled before the id was recorded.
    #[error("deduplication insert cancelled")]
    Cancelled,
    /// The index is a contract stub with no backing store yet.
    #[error("deduplication index is not implemented")]
    Unimplemented,
}

/// A set of seen transaction ids.
pub trait Index {
    /// The error produced by [`Index::insert`].
    type InsertIdError: core::fmt::Debug + core::fmt::Display;

    /// Record `tx` as seen, reporting whether it was already present.
    async fn insert(
        &mut self,
        tx: TxId,
        cancel: &CancellationToken,
    ) -> Result<Insertion, Self::InsertIdError>;
}
