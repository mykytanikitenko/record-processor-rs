//! The payments-engine contract: the transaction input stream and the
//! service trait.

use record_processor_domain::input::Transaction;
use record_processor_domain::output::AccountBalance;

/// A fallible, non-blocking source of transactions.
///
/// The stream yields [`Transaction::Eof`] immediately before terminating, so a
/// consumer can distinguish a normal end of input from a mid-stream failure.
pub trait TransactionStream {
    /// The error produced while pulling the next transaction.
    type Error: core::fmt::Debug + core::fmt::Display;

    /// Pull the next transaction, or `None` once the stream is exhausted.
    async fn next(&mut self) -> Result<Option<Transaction>, Self::Error>;
}

/// A payments engine that consumes a transaction stream and produces the
/// final per-client account balances.
pub trait Service<S: TransactionStream> {
    /// The error produced while serving.
    type Error;

    /// Consume `input` to completion and produce the account balances, ordered
    /// by client identifier.
    async fn serve(self, input: S) -> Result<Vec<AccountBalance>, Self::Error>;
}
