//! Payments-engine contract: transaction sources, balance streams, and the
//! service that pipes one into the other.

pub mod balance_stream;
pub mod serve;
pub mod service;
pub mod transaction_stream;

#[cfg(feature = "unit")]
pub use balance_stream::MockBalances;
pub use balance_stream::{
    BalanceStream, Balances, BalancesError, MockBalancesError, RealBalances, RealBalancesError,
};
pub use serve::EngineError;
pub use service::{Engine, ServeDispatchError, ServeError, Service, Type as ServiceType};
#[cfg(feature = "unit")]
pub use service::{Mock, MockServiceError};
pub use transaction_stream::TransactionStream;
