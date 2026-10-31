//! The account-balance writer: formats balances as CSV rows into an owned
//! async byte sink.

use compio::buf::IoBuf as _;
use compio::io::{AsyncWrite, AsyncWriteExt as _};
use rust_decimal::Decimal;
use service::AccountBalance;
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

/// The fixed CSV header written once before the first balance row.
const HEADER: &[u8] = b"client,available,held,total,locked\n";

/// Capacity of the reused row buffer. A balance row is at most a `u16` client
/// id, three 29-digit signed decimals, a boolean, four commas, and a newline.
const ROW_CAP: usize = 128;

/// Failure to write through the production writer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WriteError {
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
    /// The reused buffer each row is formatted into.
    row: [u8; ROW_CAP],
    /// The async byte sink the real writer targets.
    sink: S,
    /// Whether the CSV header has already been written.
    wrote_header: bool,
}

impl<S> From<S> for Writer<S> {
    #[inline]
    fn from(sink: S) -> Self {
        Self {
            row: [0_u8; ROW_CAP],
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
    /// Write the CSV header exactly once, flushing so it reaches the sink even
    /// when no balance row follows.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Cancelled`] when the token fires or
    /// [`WriteError::Io`] when the sink fails.
    #[inline]
    pub async fn write_header(&mut self, cancel: &CancellationToken) -> Result<(), WriteError> {
        if cancel.is_cancelled() {
            return Err(WriteError::Cancelled);
        }
        if !self.wrote_header {
            self.sink.write_all(HEADER).await.0?;
            self.wrote_header = true;
        }
        self.sink.flush().await?;
        Ok(())
    }

    /// Write the header (once) followed by every balance as a CSV row.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Cancelled`] when the token fires,
    /// [`WriteError::Io`] when the sink fails, or [`WriteError::Format`] when
    /// a row cannot be formatted.
    #[inline]
    pub async fn write_all(
        &mut self,
        balances: &[AccountBalance],
        cancel: &CancellationToken,
    ) -> Result<(), WriteError> {
        self.write_header(cancel).await?;
        for balance in balances {
            if cancel.is_cancelled() {
                return Err(WriteError::Cancelled);
            }
            let len = format_row(balance, &mut self.row)?;
            let bytes = self.row.slice(..len);
            self.sink.write_all(bytes).await.0?;
        }
        self.sink.flush().await?;
        Ok(())
    }
}

/// Format one account balance into `buf`, returning the number of bytes
/// written.
fn format_row(balance: &AccountBalance, buf: &mut [u8]) -> Result<usize, FormatError> {
    let mut pos = 0_usize;

    write_uint_padded(buf, &mut pos, i128::from(*balance.client()), 0_usize)
        .map_err(FormatError::from)?;
    push_byte!(buf, &mut pos, b',', FormatError::Overflow);
    write_decimal(buf, &mut pos, *balance.available()).map_err(FormatError::from)?;
    push_byte!(buf, &mut pos, b',', FormatError::Overflow);
    write_decimal(buf, &mut pos, *balance.held()).map_err(FormatError::from)?;
    push_byte!(buf, &mut pos, b',', FormatError::Overflow);
    write_decimal(buf, &mut pos, *balance.total()).map_err(FormatError::from)?;
    push_byte!(buf, &mut pos, b',', FormatError::Overflow);

    let locked: &[u8] = if balance.locked() { b"true" } else { b"false" };
    for &byte in locked {
        push_byte!(buf, &mut pos, byte, FormatError::Overflow);
    }

    push_byte!(buf, &mut pos, b'\n', FormatError::Overflow);

    Ok(pos)
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
