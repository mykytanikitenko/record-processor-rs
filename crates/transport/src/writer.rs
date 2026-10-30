//! The account-balance writer contract: an owned sink over an async byte
//! target.

use compio::buf::IoBuf as _;
use compio::io::{AsyncWrite, AsyncWriteExt as _};
use record_processor_domain::output::AccountBalance;
use rust_decimal::Decimal;
use tokio_util::sync::CancellationToken;

/// Appends one byte to the reused output buffer, failing loudly when the
/// buffer is exhausted rather than indexing past its end.
macro_rules! push_byte {
    ($buf:expr, $pos:expr, $byte:expr, $overflow:expr) => {{
        match $buf.get_mut(*$pos) {
            Some(slot) => {
                *slot = $byte;
                *$pos = (*$pos).checked_add(1).ok_or($overflow)?;
            }
            None => return Err($overflow),
        }
    }};
}

/// The fixed CSV header written once before the first balance batch.
const HEADER: &[u8] = b"client,available,held,total,locked\n";

/// Capacity of the reused batch buffer. A balance row is at most a `u16`
/// client id, three 29-digit signed decimals, a boolean, four commas, and a
/// newline; the batch buffer holds several rows in one write.
const BATCH_BUF_CAP: usize = 1024;

/// Failure to write through the production writer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WriteBatchError {
    /// The sink was cancelled before every balance was written.
    #[error("balance writer cancelled")]
    Cancelled,
    /// A balance could not be formatted into the output buffer.
    #[error(transparent)]
    Format(#[from] FormatError),
    /// The underlying byte sink failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Failure to format one account-balance row.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FormatError {
    /// A decimal could not be rendered.
    #[error(transparent)]
    Decimal(#[from] DecimalError),
    /// A formatted field overran the output buffer or a numeric range.
    #[error("balance row overran the output buffer")]
    Overflow,
    /// A decimal's integer part could not be rendered.
    #[error(transparent)]
    Uint(#[from] UintError),
}

/// Failure to render a decimal amount into the output buffer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DecimalError {
    /// The output buffer or a numeric range was exhausted.
    #[error("decimal amount overran the output buffer")]
    Overflow,
    /// The decimal scale did not fit the platform `usize`.
    #[error("decimal scale is out of range")]
    Scale(#[from] std::num::TryFromIntError),
    /// The decimal's integer part could not be rendered.
    #[error(transparent)]
    Uint(#[from] UintError),
}

/// Failure to render a non-negative integer into the output buffer.
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UintError {
    /// The output buffer was exhausted.
    #[error("integer overran the output buffer")]
    Overflow,
}

/// A production account-balance writer over an async byte sink.
#[derive(Debug)]
pub struct Writer<S> {
    /// The reused buffer every row of a batch is formatted into.
    buf: [u8; BATCH_BUF_CAP],
    /// The async byte sink the real writer targets.
    sink: S,
    /// Whether the CSV header has already been written.
    wrote_header: bool,
}

impl<S> From<S> for Writer<S> {
    #[inline]
    fn from(sink: S) -> Self {
        Self {
            buf: [0_u8; BATCH_BUF_CAP],
            sink,
            wrote_header: false,
        }
    }
}

impl<S> Writer<S> {
    /// Consume the writer, returning the underlying sink.
    #[inline]
    pub fn into_inner(self) -> S {
        self.sink
    }
}

impl<S> Writer<S>
where
    S: AsyncWrite,
{
    /// Write the header (once) followed by every balance as one CSV batch.
    ///
    /// # Errors
    ///
    /// Returns [`WriteBatchError::Cancelled`] when the token fires,
    /// [`WriteBatchError::Io`] when the sink fails, or
    /// [`WriteBatchError::Format`] when a row cannot be formatted.
    #[inline]
    pub async fn write_batch(
        &mut self,
        balances: &[AccountBalance],
        cancel: &CancellationToken,
    ) -> Result<(), WriteBatchError> {
        if cancel.is_cancelled() {
            return Err(WriteBatchError::Cancelled);
        }
        if !self.wrote_header {
            self.sink.write_all(HEADER).await.0?;
            self.wrote_header = true;
        }
        let mut pos = 0_usize;
        for balance in balances {
            format_row(balance, &mut self.buf, &mut pos)?;
        }
        self.sink.write_all(self.buf.slice(..pos)).await.0?;
        self.sink.flush().await?;
        Ok(())
    }
}

/// Format one account balance into `buf` at `pos`, advancing `pos`.
fn format_row(
    balance: &AccountBalance,
    buf: &mut [u8],
    pos: &mut usize,
) -> Result<(), FormatError> {
    write_uint_padded(buf, pos, i128::from(*balance.client()), 0_usize)
        .map_err(FormatError::from)?;
    push_byte!(buf, pos, b',', FormatError::Overflow);
    write_decimal(buf, pos, *balance.available()).map_err(FormatError::from)?;
    push_byte!(buf, pos, b',', FormatError::Overflow);
    write_decimal(buf, pos, *balance.held()).map_err(FormatError::from)?;
    push_byte!(buf, pos, b',', FormatError::Overflow);
    write_decimal(buf, pos, *balance.total()).map_err(FormatError::from)?;
    push_byte!(buf, pos, b',', FormatError::Overflow);

    let locked: &[u8] = if balance.locked() { b"true" } else { b"false" };
    for &byte in locked {
        push_byte!(buf, pos, byte, FormatError::Overflow);
    }

    push_byte!(buf, pos, b'\n', FormatError::Overflow);

    Ok(())
}

/// Renders a decimal in its shortest canonical form (normalized, no trailing
/// zeros) into `buf` at `pos`.
fn write_decimal(buf: &mut [u8], pos: &mut usize, decimal: Decimal) -> Result<(), DecimalError> {
    let normalized = decimal.normalize();
    let mut mantissa = normalized.mantissa();
    let scale = usize::try_from(normalized.scale())?;

    if mantissa < 0 {
        push_byte!(buf, pos, b'-', DecimalError::Overflow);
        mantissa = mantissa.checked_neg().ok_or(DecimalError::Overflow)?;
    }

    let mut pow10: i128 = 1;
    let mut remaining = scale;
    while remaining > 0 {
        pow10 = pow10.checked_mul(10).ok_or(DecimalError::Overflow)?;
        remaining = remaining.checked_sub(1).ok_or(DecimalError::Overflow)?;
    }

    let integer = mantissa.checked_div(pow10).ok_or(DecimalError::Overflow)?;
    let fraction = mantissa.checked_rem(pow10).ok_or(DecimalError::Overflow)?;

    write_uint_padded(buf, pos, integer, 0_usize).map_err(DecimalError::from)?;
    if scale > 0 {
        push_byte!(buf, pos, b'.', DecimalError::Overflow);
        write_uint_padded(buf, pos, fraction, scale).map_err(DecimalError::from)?;
    }

    Ok(())
}

/// Renders the decimal digits of a non-negative integer, left-padded with
/// zeros to `width` digits (no padding when `width` is zero).
fn write_uint_padded(
    buf: &mut [u8],
    pos: &mut usize,
    mut value: i128,
    width: usize,
) -> Result<(), UintError> {
    let mut scratch = [0_u8; 40];
    let mut ndigits = 0_usize;

    loop {
        let digit = match value.checked_rem(10).ok_or(UintError::Overflow)? {
            0 => b'0',
            1 => b'1',
            2 => b'2',
            3 => b'3',
            4 => b'4',
            5 => b'5',
            6 => b'6',
            7 => b'7',
            8 => b'8',
            9 => b'9',
            _ => return Err(UintError::Overflow),
        };
        let Some(slot) = scratch.get_mut(ndigits) else {
            return Err(UintError::Overflow);
        };
        *slot = digit;
        ndigits = ndigits.checked_add(1).ok_or(UintError::Overflow)?;
        value = value.checked_div(10).ok_or(UintError::Overflow)?;
        if value == 0 {
            break;
        }
    }

    let zeros = if width > ndigits {
        width.checked_sub(ndigits).ok_or(UintError::Overflow)?
    } else {
        0_usize
    };

    let mut written = 0_usize;
    while written < zeros {
        push_byte!(buf, pos, b'0', UintError::Overflow);
        written = written.checked_add(1).ok_or(UintError::Overflow)?;
    }

    while ndigits > 0 {
        ndigits = ndigits.checked_sub(1).ok_or(UintError::Overflow)?;
        let Some(&digit) = scratch.get(ndigits) else {
            return Err(UintError::Overflow);
        };
        push_byte!(buf, pos, digit, UintError::Overflow);
    }

    Ok(())
}
