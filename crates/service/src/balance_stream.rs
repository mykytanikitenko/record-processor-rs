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

/// Failure to pull from the production balance stream.
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

/// Failure to pull from the mock balance stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum MockBalancesError {
    /// The stream was cancelled before it exhausted.
    #[error("balance stream cancelled")]
    Cancelled,
}

/// Failure to pull from a dispatched balance stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum BalancesError {
    /// The mock balance stream failed.
    #[error(transparent)]
    Mock(#[from] MockBalancesError),
    /// The production balance stream failed.
    #[error(transparent)]
    Real(#[from] RealBalancesError),
}

/// An owned, named balance stream dispatched between the real engine output
/// and a unit-test mock.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Balances {
    /// The mock balance stream, enabled for unit tests only.
    #[cfg(feature = "unit")]
    Mock(MockBalances),
    /// The production balance stream (a contract stub until the engine lands).
    Real(RealBalances),
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

/// The mock balance stream, enabled for unit tests only. It yields at most one
/// balance (set at construction) and then terminates.
#[cfg(feature = "unit")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MockBalances {
    /// The single balance to yield, then `None`.
    item: Option<AccountBalance>,
}

#[cfg(feature = "unit")]
impl From<Option<AccountBalance>> for MockBalances {
    #[inline]
    fn from(item: Option<AccountBalance>) -> Self {
        Self { item }
    }
}

#[cfg(feature = "unit")]
impl BalanceStream for MockBalances {
    type Error = MockBalancesError;

    #[inline]
    async fn next(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<AccountBalance>, Self::Error> {
        if cancel.is_cancelled() {
            Err(MockBalancesError::Cancelled)
        } else {
            Ok(self.item.take())
        }
    }
}

impl BalanceStream for Balances {
    type Error = BalancesError;

    #[inline]
    async fn next(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<AccountBalance>, Self::Error> {
        match self {
            #[cfg(feature = "unit")]
            Self::Mock(inner) => inner.next(cancel).await.map_err(Into::into),
            Self::Real(inner) => inner.next(cancel).await.map_err(Into::into),
        }
    }
}
