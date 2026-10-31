//! Stdio serve pipeline: adapt the CSV reader into a [`Transactions`] slice
//! stream and drive a [`Consumer`] toward a stdout [`Writer`].

use compio::io::AsyncRead;
use service::stream::{Balances, Transactions};
use service::{Consumer, Transaction};
use tokio_util::sync::CancellationToken;

use crate::reader::{Input, ReadError, Reader};
use crate::writer::{WriteError, Writer};

/// The number of transactions buffered per [`StdioTransactions::next_batch`]
/// yield.
const BATCH: usize = 1024;

/// Adapts the CSV [`Reader`] into a [`Transactions`] slice stream.
///
/// Each batch fills a fixed, reused buffer whose slice is handed back
/// borrowing the adapter.
#[derive(Debug)]
pub struct StdioTransactions<S> {
    /// The underlying transaction reader.
    reader: Reader<S>,
    /// The reused batch buffer.
    buffer: [Transaction; BATCH],
    /// Whether the reader has reached end-of-input.
    exhausted: bool,
}

impl<S> From<Input<S>> for StdioTransactions<S> {
    #[inline]
    fn from(input: Input<S>) -> Self {
        Self {
            reader: Reader::from(input),
            buffer: [Transaction::Eof; BATCH],
            exhausted: false,
        }
    }
}

impl<S> Transactions for StdioTransactions<S>
where
    S: AsyncRead,
{
    type Error = ReadError;

    async fn next_batch<'stream>(
        &'stream mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<&'stream [Transaction]>, Self::Error> {
        if self.exhausted {
            return Ok(None);
        }

        let mut len = 0_usize;
        while len < BATCH {
            match self.reader.next(cancel).await? {
                Some(Transaction::Eof) | None => {
                    self.exhausted = true;
                    break;
                }
                Some(transaction) => match self.buffer.get_mut(len) {
                    Some(slot) => {
                        *slot = transaction;
                        len += 1;
                    }
                    None => break,
                },
            }
        }

        if len == 0 {
            return Ok(None);
        }
        match self.buffer.get(..len) {
            Some(batch) => Ok(Some(batch)),
            None => Ok(None),
        }
    }
}

/// Failure while serving the transaction stream to stdout.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServeError<C, S>
where
    C: Consumer<StdioTransactions<S>>,
    S: AsyncRead,
{
    /// Draining the transaction stream into balances failed.
    #[error("consuming the transaction stream failed: {0}")]
    Consume(C::ConsumeError),
    /// Reading the next balance batch failed.
    #[error("reading the balance stream failed: {0}")]
    Balance(<C::Balances as Balances>::Error),
    /// Writing a balance batch to stdout failed.
    #[error("writing the balance stream failed: {0}")]
    Write(WriteError),
}

/// Drive `consumer` over `input`, writing the projected balances to stdout.
///
/// # Errors
///
/// Returns [`ServeError::Consume`] when draining the transaction stream fails,
/// [`ServeError::Balance`] when reading a balance batch fails, and
/// [`ServeError::Write`] when writing a balance batch fails.
#[inline]
pub async fn serve<C, S>(
    consumer: C,
    input: Input<S>,
    cancel: &CancellationToken,
) -> Result<(), ServeError<C, S>>
where
    C: Consumer<StdioTransactions<S>>,
    S: AsyncRead,
{
    let mut transactions = StdioTransactions::from(input);
    let mut balances = consumer
        .consume(cancel, &mut transactions)
        .await
        .map_err(ServeError::Consume)?;
    let mut writer = Writer::from(compio::fs::stdout());
    loop {
        match balances
            .next_batch(cancel)
            .await
            .map_err(ServeError::Balance)?
        {
            Some(batch) => writer
                .write_all(batch, cancel)
                .await
                .map_err(ServeError::Write)?,
            None => break,
        }
    }
    Ok(())
}
