//! Stage one: append the input transaction stream to the queue in batches.

use record_processor_domain::input::Transaction;
use storage::{Append, Span};
use tokio_util::sync::CancellationToken;

use crate::engine::Engine;
use crate::transaction_stream::TransactionStream;

/// The error of pulling from the input stream `S`.
pub type SourceFailure<S> = <S as TransactionStream>::Error;

/// The error of appending a batch to the queue `Q`.
pub type AppendFailure<Q> = <Q as Append<Transaction>>::Error;

/// Failure to ingest the input stream into the queue.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum IngestError<S, A> {
    /// Appending a batch to the queue failed.
    #[error("appending a batch to the queue failed: {0}")]
    Append(A),
    /// Ingestion was cancelled before the input was exhausted.
    #[error("ingestion cancelled")]
    Cancelled,
    /// Pulling from the input stream failed.
    #[error("reading the input stream failed: {0}")]
    Source(S),
    /// The stage has no business-logic implementation yet.
    #[error("ingest stage is not implemented")]
    Unimplemented,
}

impl<Q, K> Engine<'_, Q, K>
where
    Q: Append<Transaction>,
{
    /// Drain `source` into the queue, filling `batch` and appending it whole
    /// each time it is full and once more at end of input. Returns the span of
    /// every appended record.
    ///
    /// # Errors
    ///
    /// Returns [`IngestError::Cancelled`] when the token fires,
    /// [`IngestError::Source`] or [`IngestError::Append`] when the stream or
    /// the queue fails, and [`IngestError::Unimplemented`] (contract stub).
    #[inline]
    pub async fn ingest<S>(
        &self,
        _source: &mut S,
        _batch: &mut [Transaction],
        cancel: &CancellationToken,
    ) -> Result<Span, IngestError<SourceFailure<S>, AppendFailure<Q>>>
    where
        S: TransactionStream,
    {
        if cancel.is_cancelled() {
            Err(IngestError::Cancelled)
        } else {
            Err(IngestError::Unimplemented)
        }
    }
}
