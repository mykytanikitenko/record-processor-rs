//! Payments-engine contracts and their production implementation: slice
//! streams, engine state, and the consumer that pipes one into the other.

pub mod consumer;
pub mod engine_state;
#[cfg(feature = "mocks")]
pub mod mocks;
pub mod stream;

pub use consumer::{
    ApplyError, BalanceStream, ConsumeError, Consumer, DispatchError, Engine, EngineParts, Type,
};
pub use engine_state::{
    ClientsError, DepositState, DepositStateParts, DepositStatus, KvState, LoadClientError,
    LoadDepositError, ServiceState, ServiceStateParts, State, StoreClientError, StoreDepositError,
};
pub use stream::{Balances, Transactions};

pub use record_processor_domain::client_id::ClientId;
pub use record_processor_domain::input::{Amount, ConstructionError, Transaction, TxId};
pub use record_processor_domain::output::{AccountBalance, Balance, OverflowError, Parts};
