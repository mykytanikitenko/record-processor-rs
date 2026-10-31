//! Standard-output sink.
//!
//! [`compio::fs::stdout`] wraps the process stdout descriptor in an
//! [`AsyncFd`](compio::runtime::fd::AsyncFd), which the `polling` driver
//! registers with `epoll`. `epoll` rejects regular files with `EPERM`, so
//! shell redirection (`cargo run -- x.csv > accounts.csv`) fails while piping
//! (`| cat`) succeeds. A regular-file stdout is therefore written positionally
//! through a [`compio::fs::File`] (which the driver handles without epoll),
//! while a pipe or terminal keeps the epoll-backed [`compio::fs::Stdout`].

use compio::buf::{BufResult, IoBuf};
use compio::io::{AsyncWrite, AsyncWriteAtExt as _, AsyncWriteExt};

use crate::writer::Sink;

/// Failure to construct the standard-output sink.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StdoutError {
    /// The duplicated stdout descriptor could not be opened.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Failure to write through the standard-output sink.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StdoutSinkError {
    /// The underlying stdout descriptor failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The output offset overran a numeric range.
    #[error("output offset overran a numeric range")]
    Overflow,
}

/// The standard-output sink, split by the underlying descriptor kind.
#[derive(Debug)]
pub(crate) enum Stdout {
    /// A regular-file standard output, written positionally.
    Regular {
        /// The duplicated stdout file.
        file: compio::fs::File,
        /// The next write offset.
        offset: u64,
    },
    /// A pipe or terminal standard output.
    Std(compio::fs::Stdout),
}

impl Sink for Stdout {
    type Error = StdoutSinkError;

    async fn flush(&mut self) -> Result<(), Self::Error> {
        match self {
            Self::Regular { .. } => Ok(()),
            Self::Std(stdout) => Ok(AsyncWrite::flush(stdout).await?),
        }
    }

    async fn write_all<T>(&mut self, bytes: T) -> Result<(), Self::Error>
    where
        T: IoBuf,
    {
        match self {
            Self::Regular { file, offset } => {
                let len = bytes.buf_len();
                let BufResult(result, _) = file.write_all_at(bytes, *offset).await;
                result?;
                let Ok(written) = u64::try_from(len) else {
                    return Err(StdoutSinkError::Overflow);
                };
                *offset = offset
                    .checked_add(written)
                    .ok_or(StdoutSinkError::Overflow)?;
                Ok(())
            }
            Self::Std(stdout) => {
                AsyncWriteExt::write_all(stdout, bytes).await.0?;
                Ok(())
            }
        }
    }
}

/// Detect and construct the standard-output sink.
///
/// # Errors
///
/// Returns [`StdoutError::Io`] when the duplicated stdout file cannot be
/// opened.
pub(crate) async fn stdout() -> Result<Stdout, StdoutError> {
    match compio::fs::metadata("/proc/self/fd/1").await {
        Ok(metadata) if metadata.is_file() => {
            let offset = metadata.len();
            let file = compio::fs::OpenOptions::new()
                .write(true)
                .open("/proc/self/fd/1")
                .await?;
            Ok(Stdout::Regular { file, offset })
        }
        _ => Ok(Stdout::Std(compio::fs::stdout())),
    }
}
