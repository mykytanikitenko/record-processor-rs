//! The balance-stream contract: the engine's lazy output of account balances.

use record_processor_domain::output::AccountBalance;
use tokio_util::sync::CancellationToken;

/// A source of account balances, pulled one at a time.
///
/// Cancellation is cooperative: once the passed [`CancellationToken`] fires,
/// the stream yields a typed `Cancelled` error item.
pub trait BalanceStream {
    /// The error produced while pulling the next balance.
    type Error: core::fmt::Debug + core::fmt::Display;

    /// Pull the next balance, or `None` once the stream is exhausted.
    async fn next(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<AccountBalance>, Self::Error>;
}

/// Failure to pull from the balance stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RealBalancesError {
    /// The stream was cancelled before it exhausted.
    #[error("balance stream cancelled")]
    Cancelled,
    /// The balance stream is a contract stub with no implementation yet.
    #[error("balance stream is not implemented")]
    Unimplemented,
}

/// The production balance stream. A contract stub: it yields
/// [`RealBalancesError::Unimplemented`] until the engine is implemented.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealBalances;

impl BalanceStream for RealBalances {
    type Error = RealBalancesError;

    #[inline]
    async fn next(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<AccountBalance>, Self::Error> {
        if cancel.is_cancelled() {
            Err(RealBalancesError::Cancelled)
        } else {
            Err(RealBalancesError::Unimplemented)
        }
    }
}
