//! Async CSV account balance output.

use compio::io::compat::SyncStream;
use domain::output::AccountBalance;

/// A CSV writer for account balances, backed by an asynchronous sink.
pub struct AccountBalanceWriter<S> {
    /// A sync view of the async sink, bridging the CSV writer.
    sink: SyncStream<S>,
}

/// Failure to write the account balance output.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WriterError {
    /// The CSV layer failed.
    #[error(transparent)]
    Csv(#[from] csv::Error),
    /// The underlying asynchronous I/O layer failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl<S> From<S> for AccountBalanceWriter<S> {
    #[inline]
    fn from(sink: S) -> Self {
        Self {
            sink: SyncStream::new(sink),
        }
    }
}

impl<S> AccountBalanceWriter<S> {
    /// Consume the writer, returning the underlying sink.
    #[inline]
    pub fn into_inner(self) -> S {
        self.sink.into_inner()
    }
}

impl<S> core::fmt::Debug for AccountBalanceWriter<S> {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AccountBalanceWriter")
    }
}

impl<S> AccountBalanceWriter<S>
where
    S: compio::io::AsyncWrite,
{
    /// Write every balance to the sink and flush it.
    ///
    /// # Errors
    ///
    /// Returns [`WriterError`] when a row cannot be serialized or when the
    /// underlying sink fails.
    #[inline]
    pub async fn write_all(&mut self, balances: &[AccountBalance]) -> Result<(), WriterError> {
        {
            let mut writer = csv::WriterBuilder::new()
                .has_headers(false)
                .from_writer(&mut self.sink);
            writer.write_record(["client", "available", "held", "total", "locked"])?;
            for balance in balances {
                writer.serialize(balance)?;
            }
            writer.flush()?;
        }
        let _written = self.sink.flush_write_buf().await?;
        Ok(())
    }
}
