//! Stdio serve pipeline: adapt the CSV reader into a [`Transactions`] slice
//! stream and drive a [`Consumer`] toward a stdout [`Writer`].

use allocator_api2::alloc::Allocator;
use allocator_api2::boxed::Box;
use compio::io::AsyncRead;
use service::stream::Balances;
use service::{Consumer, Transaction};
use tokio_util::sync::CancellationToken;

use crate::reader::{Input, ReadError, Reader};
use crate::stdout::{self, StdoutError, StdoutSinkError};
use crate::writer::{WriteBatchError, Writer};

/// The number of transactions buffered per [`Transactions::next_batch`]
/// yield.
const BATCH: usize = 1024;

/// A `BATCH`-sized fill of end-of-input sentinels, copied into the reused
/// buffer on construction.
static EOFFILL: [Transaction; BATCH] = [Transaction::Eof; BATCH];

/// Adapts the CSV [`Reader`] into a [`service::stream::Transactions`] slice
/// stream.
///
/// Each batch fills a reused, heap-allocated buffer whose slice is handed back
/// borrowing the adapter.
#[derive(Debug)]
pub struct Transactions<S, A>
where
    A: Allocator + Default,
{
    /// The reused batch buffer.
    buffer: Box<[Transaction], A>,
    /// Whether the reader has reached end-of-input.
    exhausted: bool,
    /// The underlying transaction reader.
    reader: Reader<S>,
}

impl<S, A> From<Input<S>> for Transactions<S, A>
where
    A: Allocator + Default,
{
    #[inline]
    fn from(input: Input<S>) -> Self {
        Self {
            buffer: Box::<[Transaction], A>::from(EOFFILL.as_slice()),
            exhausted: false,
            reader: Reader::from(input),
        }
    }
}

/// Failure to read the next transaction batch from the CSV reader.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReadBatchError {
    /// The reused batch buffer was exhausted before the batch completed.
    #[error("transaction batch buffer overflowed")]
    BufferOverflow,
    /// The underlying reader failed.
    #[error(transparent)]
    Read(#[from] ReadError),
}

impl<S, A> service::stream::Transactions for Transactions<S, A>
where
    S: AsyncRead,
    A: Allocator + Default,
{
    type Error = ReadBatchError;

    #[inline]
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
                Some(transaction) => {
                    let slot = self
                        .buffer
                        .get_mut(len)
                        .ok_or(ReadBatchError::BufferOverflow)?;
                    *slot = transaction;
                    len = len.checked_add(1).ok_or(ReadBatchError::BufferOverflow)?;
                }
            }
        }

        if len == 0 {
            return Ok(None);
        }
        let Some(batch) = self.buffer.get(..len) else {
            return Ok(None);
        };
        Ok(Some(batch))
    }
}

/// Failure while serving the transaction stream to stdout.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServeError<C, S, A>
where
    C: Consumer<Transactions<S, A>>,
    S: AsyncRead,
    A: Allocator + Default,
{
    /// Reading the next balance batch failed.
    #[error("reading the balance stream failed: {0}")]
    Balance(<C::Balances as Balances>::Error),
    /// Draining the transaction stream into balances failed.
    #[error("consuming the transaction stream failed: {0}")]
    Consume(C::ConsumeError),
    /// Opening the standard output failed.
    #[error("opening the standard output failed: {0}")]
    Stdout(StdoutError),
    /// Writing a balance batch to stdout failed.
    #[error("writing the balance stream failed: {0}")]
    Write(WriteBatchError<StdoutSinkError>),
}

/// Drive `consumer` over `input`, writing the projected balances to stdout.
///
/// # Errors
///
/// Returns [`ServeError::Consume`] when draining the transaction stream fails,
/// [`ServeError::Balance`] when reading a balance batch fails,
/// [`ServeError::Stdout`] when opening standard output fails, and
/// [`ServeError::Write`] when writing a balance batch fails.
#[inline]
pub async fn serve<C, S, A>(
    consumer: C,
    input: Input<S>,
    cancel: &CancellationToken,
) -> Result<(), ServeError<C, S, A>>
where
    C: Consumer<Transactions<S, A>>,
    S: AsyncRead,
    A: Allocator + Default,
{
    let mut transactions = Transactions::<S, A>::from(input);
    let mut balances = consumer
        .consume(cancel, &mut transactions)
        .await
        .map_err(ServeError::Consume)?;
    let mut writer = Writer::from(stdout::stdout().await.map_err(ServeError::Stdout)?);
    let mut wrote_any = false;
    while let Some(batch) = balances
        .next_batch(cancel)
        .await
        .map_err(ServeError::Balance)?
    {
        writer
            .write_all(batch, cancel)
            .await
            .map_err(ServeError::Write)?;
        wrote_any = true;
    }
    if !wrote_any {
        writer
            .write_all(&[], cancel)
            .await
            .map_err(ServeError::Write)?;
    }
    Ok(())
}
