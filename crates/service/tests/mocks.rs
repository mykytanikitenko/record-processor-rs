//! mockall doubles of the storage contracts.

use record_processor_domain::client_id::ClientId;
use record_processor_domain::input::Transaction;
use record_processor_domain::output::AccountBalance;
use storage::{Append, Entry, Lookup, Position, ReadAt, Slot, Span, Update, Walk};

/// Failure produced by the mocked queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("mocked queue failure")]
pub(crate) struct QueueFailure;

/// Failure produced by the mocked client state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("mocked client state failure")]
pub(crate) struct StateFailure;

mockall::mock! {
    pub(crate) TransactionQueue {}

    impl Append<Transaction> for TransactionQueue {
        type Error = QueueFailure;

        async fn append(&self, batch: &[Transaction]) -> Result<Span, QueueFailure>;
    }

    impl ReadAt<Transaction> for TransactionQueue {
        type Error = QueueFailure;

        async fn read(
            &self,
            from: Position,
            into: &mut [Transaction],
        ) -> Result<Span, QueueFailure>;
    }
}

mockall::mock! {
    pub(crate) ClientState {}

    impl Lookup<ClientId, AccountBalance> for ClientState {
        type Error = StateFailure;

        async fn read(&self, key: ClientId) -> Result<Option<AccountBalance>, StateFailure>;
    }

    impl Update<ClientId, AccountBalance> for ClientState {
        type Error = StateFailure;

        async fn update(&self, key: ClientId, value: &AccountBalance) -> Result<(), StateFailure>;
    }

    impl Walk<ClientId, AccountBalance> for ClientState {
        type Error = StateFailure;

        async fn next_entry(
            &self,
            from: Slot,
        ) -> Result<Option<Entry<ClientId, AccountBalance>>, StateFailure>;
    }
}
