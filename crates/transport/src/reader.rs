//! The transaction reader contract: an owned, batched stream over an async
//! byte source.

use compio::buf::{BufResult, IntoInner as _, IoBufMut as _};
use compio::io::{AsyncRead, AsyncReadAt as _, AsyncReadExt as _};
use record_processor_domain::client_id::ClientId;
use record_processor_domain::input::{Amount, ConstructionError, Transaction, TxId};
use rust_decimal::Decimal;
use service::{TransactionBatch, TransactionStream};
use std::str::FromStr as _;
use tokio_util::sync::CancellationToken;

/// Maps the `amount` column of a `deposit` or `withdrawal` row onto an
/// [`Amount`], rejecting a missing or empty value.
macro_rules! require_amount {
    ($amount_str:expr) => {{
        let raw = $amount_str
            .filter(|field| !field.is_empty())
            .ok_or(RecordError::MissingAmount)?;
        let decimal = Decimal::from_str(raw).map_err(RecordError::InvalidDecimal)?;
        Amount::try_from(decimal)?
    }};
}

/// Rejects a non-empty `amount` column on a row kind that must not carry one.
macro_rules! reject_amount {
    ($amount_str:expr) => {{
        if $amount_str.filter(|field| !field.is_empty()).is_some() {
            return Err(RecordError::UnexpectedAmount);
        }
    }};
}

/// Extra bytes reserved on the read buffer before each refill.
const READ_CHUNK: usize = 64;

/// Maximum transactions yielded in one batch (the reused parsed array).
const BATCH_CAP: usize = 32;

/// Failure to pull the next batch from the production reader.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NextBatchError {
    /// A batch contained more rows than the reused array holds.
    #[error("transaction batch overflowed the reused array")]
    BatchOverflow,
    /// The stream was cancelled before it exhausted.
    #[error("transaction reader cancelled")]
    Cancelled,
    /// Refilling the buffer from the byte source failed.
    #[error(transparent)]
    Fill(#[from] FillError),
    /// A CSV record could not be mapped onto a transaction.
    #[error(transparent)]
    Record(#[from] RecordError),
}

/// Failure to refill the read buffer from the byte source.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FillError {
    /// The underlying byte source failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The running file offset overflowed `u64`.
    #[error("input file offset overflowed")]
    OffsetOverflow,
    /// A read length did not fit the `u64` file offset.
    #[error("input read length does not fit the file offset: {0}")]
    OffsetWiden(#[from] std::num::TryFromIntError),
}

/// Failure to map one CSV line onto a [`Transaction`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RecordError {
    /// The amount failed the domain's positivity and precision rules.
    #[error("invalid amount: {0}")]
    Amount(#[from] ConstructionError),
    /// The client column was not a valid `u16`.
    #[error("invalid client id: {0}")]
    Client(std::num::ParseIntError),
    /// The header row was absent or did not match the contract.
    #[error("CSV header must be exactly `type,client,tx,amount`")]
    Header,
    /// The amount column was not a valid decimal.
    #[error("invalid amount: {0}")]
    InvalidDecimal(rust_decimal::Error),
    /// A `deposit` or `withdrawal` row carried no amount.
    #[error("deposit or withdrawal is missing its amount")]
    MissingAmount,
    /// A transaction row was missing a required column.
    #[error("transaction row is missing a required column")]
    MissingField,
    /// A transaction row carried more than four columns.
    #[error("transaction row has too many columns")]
    TooManyFields,
    /// The transaction column was not a valid `u32`.
    #[error("invalid transaction id: {0}")]
    Tx(std::num::ParseIntError),
    /// A `dispute`, `resolve`, or `chargeback` row carried an amount.
    #[error("dispute, resolve, or chargeback must not carry an amount")]
    UnexpectedAmount,
    /// The `type` column matched no known transaction kind.
    #[error("unknown transaction type")]
    UnknownType,
    /// A line was not valid UTF-8.
    #[error("CSV record is not valid UTF-8")]
    Utf8(#[from] std::str::Utf8Error),
}

/// The owned byte source of the production reader.
///
/// `compio::fs::File` is positional (`AsyncReadAt`) and has no sequential
/// `AsyncRead`, so a file is read through an explicit offset owned here instead
/// of an adapter; both variants refill the same reused buffer.
#[derive(Debug)]
#[non_exhaustive]
pub enum Input<S> {
    /// A positional file read from `offset` onward.
    File {
        /// The file being streamed.
        file: compio::fs::File,
        /// The position of the next read.
        offset: u64,
    },
    /// A sequential stream such as standard input.
    Stream(S),
}

impl<S> From<compio::fs::File> for Input<S> {
    #[inline]
    fn from(file: compio::fs::File) -> Self {
        Self::File { file, offset: 0 }
    }
}

/// A production transaction reader over an owned byte source.
///
/// The record loop is hand-written rather than a `compio::io::framed::Framed`
/// stream: its `Stream` impl demands `R: AsyncRead + 'static`, which a generic
/// source cannot name without the forbidden `'static` token, and the header
/// state, the end-of-input marker and the per-call cancellation check have no
/// place in a `Framer`/`Decoder` pair. Batches are pulled rather than a
/// lending `Stream`, so the reused buffers can be borrowed zero-copy.
#[derive(Debug)]
pub struct Reader<S> {
    /// Bytes read but not yet consumed as complete records.
    buffer: Vec<u8>,
    /// Bytes in `buffer` already yielded by the previous batch, drained on the
    /// next pull once their borrow has ended.
    consumed: usize,
    /// Whether the source has reached end-of-input.
    eof: bool,
    /// Whether the end-of-input marker has been yielded.
    eof_yielded: bool,
    /// Whether the header row has been consumed.
    header_seen: bool,
    /// The reused array of parsed transactions for the current batch.
    parsed: [Transaction; BATCH_CAP],
    /// The owned byte source the real reader consumes.
    source: Input<S>,
}

impl<S> From<Input<S>> for Reader<S> {
    #[inline]
    fn from(source: Input<S>) -> Self {
        Self {
            buffer: Vec::from([]),
            consumed: 0_usize,
            eof: false,
            eof_yielded: false,
            header_seen: false,
            parsed: [Transaction::Eof; BATCH_CAP],
            source,
        }
    }
}

impl<S> Reader<S> {
    /// Consume the reader, returning the underlying source.
    #[inline]
    pub fn into_inner(self) -> Input<S> {
        self.source
    }
}

impl<S> Reader<S>
where
    S: AsyncRead,
{
    /// Append the next chunk of the source to the reused buffer, returning the
    /// number of bytes read (zero at end-of-input).
    async fn fill(&mut self) -> Result<usize, FillError> {
        let mut buffer = Vec::from([]);
        core::mem::swap(&mut buffer, &mut self.buffer);
        buffer.reserve(READ_CHUNK);
        match &mut self.source {
            Input::File { file, offset } => {
                let BufResult(result, tail) = file.read_at(buffer.uninit(), *offset).await;
                self.buffer = tail.into_inner();
                let read = result?;
                *offset = offset
                    .checked_add(u64::try_from(read)?)
                    .ok_or(FillError::OffsetOverflow)?;
                Ok(read)
            }
            Input::Stream(stream) => {
                let appended = stream.append(buffer).await;
                self.buffer = appended.1;
                Ok(appended.0?)
            }
        }
    }
}

impl<S> TransactionStream for Reader<S>
where
    S: AsyncRead,
{
    type Error = NextBatchError;

    #[inline]
    async fn next_batch(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<TransactionBatch<'_>>, Self::Error> {
        if cancel.is_cancelled() {
            return Err(NextBatchError::Cancelled);
        }
        // Reclaim the bytes the previous batch borrowed; its lifetime has ended
        // by the time this call re-enters.
        self.buffer.drain(..self.consumed).for_each(drop);
        self.consumed = 0_usize;

        loop {
            // Parse every complete (newline-terminated) row into the reused
            // array, recording how many bytes were consumed.
            let mut count = 0_usize;
            let mut consumed = 0_usize;
            while let Some(relative) = self
                .buffer
                .get(consumed..)
                .and_then(|tail| tail.iter().position(|&byte| byte == b'\n'))
            {
                let newline = consumed
                    .checked_add(relative)
                    .ok_or(NextBatchError::BatchOverflow)?;
                let payload = match newline.checked_sub(1) {
                    Some(prev) if self.buffer.get(prev) == Some(&b'\r') => prev,
                    _ => newline,
                };
                let line = self
                    .buffer
                    .get(consumed..payload)
                    .ok_or(NextBatchError::BatchOverflow)?;
                let record =
                    parse_record(line, &mut self.header_seen).map_err(NextBatchError::Record)?;
                if let Some(transaction) = record {
                    *self
                        .parsed
                        .get_mut(count)
                        .ok_or(NextBatchError::BatchOverflow)? = transaction;
                    count = count.checked_add(1).ok_or(NextBatchError::BatchOverflow)?;
                }
                consumed = newline
                    .checked_add(1)
                    .ok_or(NextBatchError::BatchOverflow)?;
            }
            self.consumed = consumed;

            if count > 0 {
                let raw = self
                    .buffer
                    .get(..self.consumed)
                    .ok_or(NextBatchError::BatchOverflow)?;
                let transactions = self
                    .parsed
                    .get(..count)
                    .ok_or(NextBatchError::BatchOverflow)?;
                return Ok(Some(TransactionBatch { raw, transactions }));
            }

            if self.eof {
                if !self.buffer.is_empty() {
                    // Trailing row (no trailing newline): parse it as the last
                    // record, then fall through to the end-of-input marker.
                    let record = parse_record(self.buffer.as_slice(), &mut self.header_seen)
                        .map_err(NextBatchError::Record)?;
                    self.consumed = self.buffer.len();
                    if let Some(transaction) = record {
                        *self
                            .parsed
                            .get_mut(0)
                            .ok_or(NextBatchError::BatchOverflow)? = transaction;
                        let raw = self
                            .buffer
                            .get(..self.consumed)
                            .ok_or(NextBatchError::BatchOverflow)?;
                        let transactions =
                            self.parsed.get(..1).ok_or(NextBatchError::BatchOverflow)?;
                        return Ok(Some(TransactionBatch { raw, transactions }));
                    }
                    self.buffer.drain(..).for_each(drop);
                    self.consumed = 0_usize;
                    continue;
                }
                if self.eof_yielded {
                    return Ok(None);
                }
                self.eof_yielded = true;
                *self
                    .parsed
                    .get_mut(0)
                    .ok_or(NextBatchError::BatchOverflow)? = Transaction::Eof;
                let transactions = self.parsed.get(..1).ok_or(NextBatchError::BatchOverflow)?;
                return Ok(Some(TransactionBatch {
                    raw: &[],
                    transactions,
                }));
            }

            if self.fill().await.map_err(NextBatchError::Fill)? == 0 {
                self.eof = true;
            }
        }
    }
}

/// Parse one CSV line (already freed of its trailing `\r\n`) into a
/// transaction, consuming the header row on first call.
fn parse_record(line: &[u8], header_seen: &mut bool) -> Result<Option<Transaction>, RecordError> {
    let text = core::str::from_utf8(line)?;
    let mut fields = text.split(',');

    if !*header_seen {
        *header_seen = true;
        let kind = fields.next().ok_or(RecordError::Header)?.trim();
        let client = fields.next().ok_or(RecordError::Header)?.trim();
        let tx = fields.next().ok_or(RecordError::Header)?.trim();
        let amount = fields.next().ok_or(RecordError::Header)?.trim();
        if fields.next().is_some() {
            return Err(RecordError::Header);
        }
        if kind == "type" && client == "client" && tx == "tx" && amount == "amount" {
            return Ok(None);
        }
        return Err(RecordError::Header);
    }

    let kind = fields.next().ok_or(RecordError::MissingField)?.trim();
    let client_str = fields.next().ok_or(RecordError::MissingField)?.trim();
    let tx_str = fields.next().ok_or(RecordError::MissingField)?.trim();
    let amount_str = fields.next().map(str::trim);
    if fields.next().is_some() {
        return Err(RecordError::TooManyFields);
    }

    let client = ClientId::from(client_str.parse::<u16>().map_err(RecordError::Client)?);
    let tx = TxId::from(tx_str.parse::<u32>().map_err(RecordError::Tx)?);

    let transaction = match kind {
        "deposit" => {
            let amount = require_amount!(amount_str);
            Transaction::Deposit { client, tx, amount }
        }
        "withdrawal" => {
            let amount = require_amount!(amount_str);
            Transaction::Withdrawal { client, tx, amount }
        }
        "dispute" => {
            reject_amount!(amount_str);
            Transaction::Dispute { client, tx }
        }
        "resolve" => {
            reject_amount!(amount_str);
            Transaction::Resolve { client, tx }
        }
        "chargeback" => {
            reject_amount!(amount_str);
            Transaction::Chargeback { client, tx }
        }
        _ => return Err(RecordError::UnknownType),
    };

    Ok(Some(transaction))
}
