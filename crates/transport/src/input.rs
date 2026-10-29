//! Async CSV transaction input.

use std::io::{ErrorKind, Read as _};

use compio::io::compat::SyncStream;
use domain::input::Transaction;
use service::TransactionStream;

/// A streaming CSV reader that yields [`Transaction`] values.
///
/// The asynchronous source is drained into memory on the first pull, then
/// parsed one record at a time. Draining bridges the synchronous CSV parser to
/// the asynchronous source through a [`SyncStream`], retrying on `WouldBlock`.
pub struct TransactionReader<S> {
    /// The parser, initialised once the source has been drained.
    csv: Option<csv::Reader<std::io::Cursor<Vec<u8>>>>,
    /// Whether the terminal `Eof` has already been emitted.
    finished: bool,
    /// Reused record buffer.
    record: csv::StringRecord,
    /// A sync view of the async source.
    source: SyncStream<S>,
}

/// Failure to read the transaction stream.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReaderError {
    /// The CSV layer failed.
    #[error(transparent)]
    Csv(#[from] csv::Error),
    /// The underlying asynchronous I/O layer failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl<S> From<S> for TransactionReader<S> {
    #[inline]
    fn from(source: S) -> Self {
        Self {
            csv: None,
            finished: false,
            record: csv::StringRecord::new(),
            source: SyncStream::new(source),
        }
    }
}

impl<S> core::fmt::Debug for TransactionReader<S> {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TransactionReader")
    }
}

impl<S> TransactionStream for TransactionReader<S>
where
    S: compio::io::AsyncRead,
{
    type Error = ReaderError;

    #[inline]
    async fn next(&mut self) -> Result<Option<Transaction>, Self::Error> {
        if self.finished {
            return Ok(None);
        }
        if self.csv.is_none() {
            let mut bytes = Vec::new();
            let mut buf = [0_u8; 8192];
            loop {
                match self.source.read(&mut buf) {
                    Ok(0) => break,
                    Ok(read) => {
                        let Some(chunk) = buf.get(..read) else {
                            return Err(ReaderError::Io(std::io::Error::other(
                                "read returned more bytes than the buffer holds",
                            )));
                        };
                        bytes.extend_from_slice(chunk);
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        let _read = self.source.fill_read_buf().await?;
                    }
                    Err(error) => return Err(ReaderError::Io(error)),
                }
            }
            self.csv = Some(
                csv::ReaderBuilder::new()
                    .trim(csv::Trim::All)
                    .flexible(true)
                    .from_reader(std::io::Cursor::new(bytes)),
            );
        }
        let Some(csv) = self.csv.as_mut() else {
            return Ok(None);
        };
        match csv.read_record(&mut self.record) {
            Ok(true) => {
                let headers = csv.headers()?;
                let transaction: Transaction = self.record.deserialize(Some(headers))?;
                Ok(Some(transaction))
            }
            Ok(false) => {
                self.finished = true;
                Ok(Some(Transaction::Eof))
            }
            Err(error) => Err(ReaderError::Csv(error)),
        }
    }
}
