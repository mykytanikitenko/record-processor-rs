//! Payments-engine contract: transaction sources, balance streams, and the
//! service that pipes one into the other.

pub mod balance_stream;
pub mod serve;
pub mod service;
pub mod transaction_stream;

pub use balance_stream::{BalanceStream, RealBalances, RealBalancesError};
pub use serve::EngineError;
pub use service::{Engine, ServeError, Service};
pub use transaction_stream::{TransactionBatch, TransactionStream};
