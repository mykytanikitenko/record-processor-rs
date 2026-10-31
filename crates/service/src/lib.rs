//! Payments-engine stream contracts: the transaction source the engine reads
//! and the balance stream it produces.

pub mod balance_stream;
pub mod transaction_stream;

pub use balance_stream::BalanceStream;
pub use transaction_stream::TransactionStream;

pub use record_processor_domain::client_id::ClientId;
pub use record_processor_domain::input::{Amount, ConstructionError, Transaction, TxId};
pub use record_processor_domain::output::{AccountBalance, Balance, Parts};
