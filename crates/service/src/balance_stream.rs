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
