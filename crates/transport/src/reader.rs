//! The transaction reader contract: an owned stream over an async byte source,
//! dispatched between the real reader and a unit-test mock.

use compio::io::{AsyncRead, AsyncReadExt as _};
use record_processor_domain::client_id::ClientId;
use record_processor_domain::input::{Amount, ConstructionError, Transaction, TxId};
use rust_decimal::Decimal;
use service::TransactionStream;
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

/// Failure to read from the production reader.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReadError {
    /// The stream was cancelled before it exhausted.
    #[error("transaction reader cancelled")]
    Cancelled,
    /// The underlying byte source failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A CSV record could not be mapped onto a transaction.
    #[error(transparent)]
    Record(#[from] RecordError),
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

/// Failure to read from the mock reader.
#[cfg(feature = "unit")]
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum MockReadError {
    /// The stream was cancelled before it exhausted.
    #[error("transaction reader cancelled")]
    Cancelled,
}

/// Failure to read from a dispatched reader.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReadDispatchError {
    /// The mock reader failed.
    #[cfg(feature = "unit")]
    #[error(transparent)]
    Mock(#[from] MockReadError),
    /// The production reader failed.
    #[error(transparent)]
    Real(#[from] ReadError),
}

/// A production transaction reader over an async byte source.
#[derive(Debug)]
pub struct Reader<S> {
    /// Bytes read but not yet consumed as a complete record.
    buffer: Vec<u8>,
    /// Whether the source has reached end-of-input.
    eof: bool,
    /// Whether the end-of-input marker has been yielded.
    eof_yielded: bool,
    /// Whether the header row has been consumed.
    header_seen: bool,
    /// The async byte source the real reader consumes.
    source: S,
}

impl<S> From<S> for Reader<S> {
    #[inline]
    fn from(source: S) -> Self {
        Self {
            buffer: Vec::from([]),
            eof: false,
            eof_yielded: false,
            header_seen: false,
            source,
        }
    }
}

impl<S> Reader<S> {
    /// Consume the reader, returning the underlying source.
    #[inline]
    pub fn into_inner(self) -> S {
        self.source
    }
}

impl<S> TransactionStream for Reader<S>
where
    S: AsyncRead,
{
    type Error = ReadError;

    #[inline]
    async fn next(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<Transaction>, Self::Error> {
        if cancel.is_cancelled() {
            return Err(ReadError::Cancelled);
        }
        loop {
            if let Some(newline) = self.buffer.iter().position(|&byte| byte == b'\n') {
                let payload = match newline.checked_sub(1) {
                    Some(prev) if self.buffer.get(prev) == Some(&b'\r') => prev,
                    _ => newline,
                };
                let record = parse_record(self.buffer.split_at(payload).0, &mut self.header_seen)
                    .map_err(ReadError::Record)?;
                self.buffer.drain(..=newline).for_each(drop);
                if let Some(transaction) = record {
                    return Ok(Some(transaction));
                }
                continue;
            }

            if self.eof {
                if self.buffer.is_empty() {
                    if self.eof_yielded {
                        return Ok(None);
                    }
                    self.eof_yielded = true;
                    return Ok(Some(Transaction::Eof));
                }
                let record =
                    parse_record(&self.buffer, &mut self.header_seen).map_err(ReadError::Record)?;
                self.buffer.drain(..).for_each(drop);
                if let Some(transaction) = record {
                    return Ok(Some(transaction));
                }
                continue;
            }

            let mut buffer = Vec::from([]);
            core::mem::swap(&mut buffer, &mut self.buffer);
            buffer.reserve(READ_CHUNK);
            let appended = self.source.append(buffer).await;
            self.buffer = appended.1;
            match appended.0 {
                Ok(0) => self.eof = true,
                Ok(_) => {}
                Err(error) => return Err(ReadError::Io(error)),
            }
        }
    }
}

/// The mock transaction reader, enabled for unit tests only. It yields at most
/// one transaction (set at construction) and then terminates.
#[cfg(feature = "unit")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mock {
    /// The single transaction to yield, then `None`.
    item: Option<Transaction>,
}

#[cfg(feature = "unit")]
impl From<Option<Transaction>> for Mock {
    #[inline]
    fn from(item: Option<Transaction>) -> Self {
        Self { item }
    }
}

#[cfg(feature = "unit")]
impl TransactionStream for Mock {
    type Error = MockReadError;

    #[inline]
    async fn next(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<Transaction>, Self::Error> {
        if cancel.is_cancelled() {
            Err(MockReadError::Cancelled)
        } else {
            Ok(self.item.take())
        }
    }
}

/// Dispatch between a real reader and a mock reader.
#[non_exhaustive]
#[derive(Debug)]
pub enum Type<S> {
    /// The mock reader, enabled for unit tests only.
    #[cfg(feature = "unit")]
    Mock(Mock),
    /// The production reader.
    Real(Reader<S>),
}

impl<S> TransactionStream for Type<S>
where
    S: AsyncRead,
{
    type Error = ReadDispatchError;

    #[inline]
    async fn next(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<Transaction>, Self::Error> {
        match self {
            #[cfg(feature = "unit")]
            Self::Mock(inner) => inner.next(cancel).await.map_err(Into::into),
            Self::Real(inner) => inner.next(cancel).await.map_err(Into::into),
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
