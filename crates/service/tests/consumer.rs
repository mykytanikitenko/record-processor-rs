//! Consumer unit tests against a real in-memory queue and mocked state.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use allocator_api2::alloc::Global;
    use mockall::predicate::{always, eq};
    use record_processor_domain::client_id::ClientId;
    use record_processor_domain::input::{Amount, Transaction, TxId};
    use rust_decimal::Decimal;
    use service::engine_state::MockState;
    use service::mocks::{
        MockLoadClientError, MockLoadDepositError, MockStoreClientError, MockStoreDepositError,
    };
    use service::{
        ConsumeError, Consumer as _, DepositState, DepositStateParts, DepositStatus, Engine,
        EngineParts, ServiceState, ServiceStateParts,
    };
    use std::str::FromStr as _;
    use storage::memory_queue::{NextBatchError, Parts};
    use storage::{MemoryQueue, Offset, Queue as _};
    use tokio_util::sync::CancellationToken;

    /// Build a deposit transaction.
    fn deposit(client: u16, tx: u32, amount: &str) -> anyhow::Result<Transaction> {
        Ok(Transaction::Deposit {
            client: ClientId::from(client),
            tx: TxId::from(tx),
            amount: Amount::try_from(Decimal::from_str(amount)?)?,
        })
    }

    /// Build a withdrawal transaction.
    fn withdrawal(client: u16, tx: u32, amount: &str) -> anyhow::Result<Transaction> {
        Ok(Transaction::Withdrawal {
            client: ClientId::from(client),
            tx: TxId::from(tx),
            amount: Amount::try_from(Decimal::from_str(amount)?)?,
        })
    }

    /// Build a dispute transaction.
    fn dispute(client: u16, tx: u32) -> Transaction {
        Transaction::Dispute {
            client: ClientId::from(client),
            tx: TxId::from(tx),
        }
    }

    /// Build a resolve transaction.
    fn resolve(client: u16, tx: u32) -> Transaction {
        Transaction::Resolve {
            client: ClientId::from(client),
            tx: TxId::from(tx),
        }
    }

    /// Build a chargeback transaction.
    fn chargeback(client: u16, tx: u32) -> Transaction {
        Transaction::Chargeback {
            client: ClientId::from(client),
            tx: TxId::from(tx),
        }
    }

    /// Build a service state.
    fn service_state(available: &str, held: &str, locked: bool) -> anyhow::Result<ServiceState> {
        Ok(ServiceState::from(ServiceStateParts {
            available: Decimal::from_str(available)?,
            held: Decimal::from_str(held)?,
            locked,
        }))
    }

    /// Build a deposit state.
    fn deposit_state(
        client: u16,
        amount: &str,
        status: DepositStatus,
    ) -> anyhow::Result<DepositState> {
        Ok(DepositState::from(DepositStateParts {
            amount: Decimal::from_str(amount)?,
            client: ClientId::from(client),
            status,
        }))
    }

    /// Append `rows` to a fresh queue and return it.
    async fn queue(rows: Vec<Transaction>) -> anyhow::Result<MemoryQueue<Transaction, Global>> {
        let queue = MemoryQueue::from(Parts { alloc: Global });
        let offset = queue.append(&rows, &CancellationToken::new()).await?;
        anyhow::ensure!(
            offset == Offset::from(0_u64),
            "unexpected offset {offset:?}"
        );
        Ok(queue)
    }

    #[rudzio::test]
    async fn deposit_credits_new_account(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let stored_deposit = deposit_state(1, "1.5", DepositStatus::Available)?;
        let stored_client = service_state("1.5", "0.0", false)?;
        let queue = queue(vec![deposit(1, 1, "1.5")?, Transaction::Eof]).await?;
        let mut state = MockState::new();
        let _load_deposit = state
            .expect_load_deposit()
            .with(eq(TxId::from(1)), always())
            .times(1)
            .returning(
                |_tx, _cancel| -> Result<Option<DepositState>, MockLoadDepositError> { Ok(None) },
            );
        let _store_deposit = state
            .expect_store_deposit()
            .with(eq(TxId::from(1)), eq(stored_deposit), always())
            .times(1)
            .returning(|_tx, _state, _cancel| -> Result<(), MockStoreDepositError> { Ok(()) });
        let _load_client = state
            .expect_load_client()
            .with(eq(ClientId::from(1)), always())
            .times(1)
            .returning(
                |_client, _cancel| -> Result<Option<ServiceState>, MockLoadClientError> {
                    Ok(None)
                },
            );
        let _store_client = state
            .expect_store_client()
            .with(eq(ClientId::from(1)), eq(stored_client), always())
            .times(1)
            .returning(|_client, _state, _cancel| -> Result<(), MockStoreClientError> { Ok(()) });
        let engine = Engine::from(EngineParts { queue, state });
        engine.consume(&CancellationToken::new()).await?;
        Ok(())
    }

    #[rudzio::test]
    async fn withdrawal_debits_sufficient_funds(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let loaded = service_state("10.0", "0.0", false)?;
        let stored = service_state("6.0", "0.0", false)?;
        let queue = queue(vec![withdrawal(1, 1, "4.0")?, Transaction::Eof]).await?;
        let mut state = MockState::new();
        let _load_client = state
            .expect_load_client()
            .with(eq(ClientId::from(1)), always())
            .times(1)
            .returning(
                move |_client, _cancel| -> Result<Option<ServiceState>, MockLoadClientError> {
                    Ok(Some(loaded))
                },
            );
        let _store_client = state
            .expect_store_client()
            .with(eq(ClientId::from(1)), eq(stored), always())
            .times(1)
            .returning(|_client, _state, _cancel| -> Result<(), MockStoreClientError> { Ok(()) });
        let engine = Engine::from(EngineParts { queue, state });
        engine.consume(&CancellationToken::new()).await?;
        Ok(())
    }

    #[rudzio::test]
    async fn withdrawal_insufficient_is_ignored(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let loaded = service_state("10.0", "0.0", false)?;
        let queue = queue(vec![withdrawal(1, 1, "20.0")?, Transaction::Eof]).await?;
        let mut state = MockState::new();
        let _load_client = state
            .expect_load_client()
            .with(eq(ClientId::from(1)), always())
            .times(1)
            .returning(
                move |_client, _cancel| -> Result<Option<ServiceState>, MockLoadClientError> {
                    Ok(Some(loaded))
                },
            );
        let engine = Engine::from(EngineParts { queue, state });
        engine.consume(&CancellationToken::new()).await?;
        Ok(())
    }

    #[rudzio::test]
    async fn withdrawal_new_client_is_ignored(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = queue(vec![withdrawal(1, 1, "5.0")?, Transaction::Eof]).await?;
        let mut state = MockState::new();
        let _load_client = state
            .expect_load_client()
            .with(eq(ClientId::from(1)), always())
            .times(1)
            .returning(
                |_client, _cancel| -> Result<Option<ServiceState>, MockLoadClientError> {
                    Ok(None)
                },
            );
        let engine = Engine::from(EngineParts { queue, state });
        engine.consume(&CancellationToken::new()).await?;
        Ok(())
    }

    #[rudzio::test]
    async fn dispute_moves_available_to_held(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let available = deposit_state(1, "10.0", DepositStatus::Available)?;
        let disputed = deposit_state(1, "10.0", DepositStatus::Disputed)?;
        let loaded = service_state("10.0", "0.0", false)?;
        let stored = service_state("0.0", "10.0", false)?;
        let queue = queue(vec![dispute(1, 1), Transaction::Eof]).await?;
        let mut state = MockState::new();
        let _load_deposit = state
            .expect_load_deposit()
            .with(eq(TxId::from(1)), always())
            .times(1)
            .returning(
                move |_tx, _cancel| -> Result<Option<DepositState>, MockLoadDepositError> {
                    Ok(Some(available))
                },
            );
        let _store_deposit = state
            .expect_store_deposit()
            .with(eq(TxId::from(1)), eq(disputed), always())
            .times(1)
            .returning(|_tx, _state, _cancel| -> Result<(), MockStoreDepositError> { Ok(()) });
        let _load_client = state
            .expect_load_client()
            .with(eq(ClientId::from(1)), always())
            .times(1)
            .returning(
                move |_client, _cancel| -> Result<Option<ServiceState>, MockLoadClientError> {
                    Ok(Some(loaded))
                },
            );
        let _store_client = state
            .expect_store_client()
            .with(eq(ClientId::from(1)), eq(stored), always())
            .times(1)
            .returning(|_client, _state, _cancel| -> Result<(), MockStoreClientError> { Ok(()) });
        let engine = Engine::from(EngineParts { queue, state });
        engine.consume(&CancellationToken::new()).await?;
        Ok(())
    }

    #[rudzio::test]
    async fn resolve_releases_held_to_available(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let disputed = deposit_state(1, "10.0", DepositStatus::Disputed)?;
        let available = deposit_state(1, "10.0", DepositStatus::Available)?;
        let loaded = service_state("0.0", "10.0", false)?;
        let stored = service_state("10.0", "0.0", false)?;
        let queue = queue(vec![resolve(1, 1), Transaction::Eof]).await?;
        let mut state = MockState::new();
        let _load_deposit = state
            .expect_load_deposit()
            .with(eq(TxId::from(1)), always())
            .times(1)
            .returning(
                move |_tx, _cancel| -> Result<Option<DepositState>, MockLoadDepositError> {
                    Ok(Some(disputed))
                },
            );
        let _store_deposit = state
            .expect_store_deposit()
            .with(eq(TxId::from(1)), eq(available), always())
            .times(1)
            .returning(|_tx, _state, _cancel| -> Result<(), MockStoreDepositError> { Ok(()) });
        let _load_client = state
            .expect_load_client()
            .with(eq(ClientId::from(1)), always())
            .times(1)
            .returning(
                move |_client, _cancel| -> Result<Option<ServiceState>, MockLoadClientError> {
                    Ok(Some(loaded))
                },
            );
        let _store_client = state
            .expect_store_client()
            .with(eq(ClientId::from(1)), eq(stored), always())
            .times(1)
            .returning(|_client, _state, _cancel| -> Result<(), MockStoreClientError> { Ok(()) });
        let engine = Engine::from(EngineParts { queue, state });
        engine.consume(&CancellationToken::new()).await?;
        Ok(())
    }

    #[rudzio::test]
    async fn chargeback_drains_held_and_locks(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let disputed = deposit_state(1, "10.0", DepositStatus::Disputed)?;
        let charged_back = deposit_state(1, "10.0", DepositStatus::ChargedBack)?;
        let loaded = service_state("0.0", "10.0", false)?;
        let stored = service_state("0.0", "0.0", true)?;
        let queue = queue(vec![chargeback(1, 1), Transaction::Eof]).await?;
        let mut state = MockState::new();
        let _load_deposit = state
            .expect_load_deposit()
            .with(eq(TxId::from(1)), always())
            .times(1)
            .returning(
                move |_tx, _cancel| -> Result<Option<DepositState>, MockLoadDepositError> {
                    Ok(Some(disputed))
                },
            );
        let _store_deposit = state
            .expect_store_deposit()
            .with(eq(TxId::from(1)), eq(charged_back), always())
            .times(1)
            .returning(|_tx, _state, _cancel| -> Result<(), MockStoreDepositError> { Ok(()) });
        let _load_client = state
            .expect_load_client()
            .with(eq(ClientId::from(1)), always())
            .times(1)
            .returning(
                move |_client, _cancel| -> Result<Option<ServiceState>, MockLoadClientError> {
                    Ok(Some(loaded))
                },
            );
        let _store_client = state
            .expect_store_client()
            .with(eq(ClientId::from(1)), eq(stored), always())
            .times(1)
            .returning(|_client, _state, _cancel| -> Result<(), MockStoreClientError> { Ok(()) });
        let engine = Engine::from(EngineParts { queue, state });
        engine.consume(&CancellationToken::new()).await?;
        Ok(())
    }

    #[rudzio::test]
    async fn dispute_unknown_tx_is_ignored(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = queue(vec![dispute(1, 999), Transaction::Eof]).await?;
        let mut state = MockState::new();
        let _load_deposit = state
            .expect_load_deposit()
            .with(eq(TxId::from(999)), always())
            .times(1)
            .returning(
                |_tx, _cancel| -> Result<Option<DepositState>, MockLoadDepositError> { Ok(None) },
            );
        let engine = Engine::from(EngineParts { queue, state });
        engine.consume(&CancellationToken::new()).await?;
        Ok(())
    }

    #[rudzio::test]
    async fn cancellation_ends_waiting_for_next_batch(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = MemoryQueue::from(Parts { alloc: Global });
        let state = MockState::new();
        let engine = Engine::from(EngineParts { queue, state });
        let cancel = CancellationToken::new();
        cancel.cancel();
        let consumed = engine.consume(&cancel).await;
        anyhow::ensure!(
            matches!(
                consumed,
                Err(ConsumeError::NextBatch(NextBatchError::Cancelled))
            ),
            "unexpected consume {consumed:?}"
        );
        Ok(())
    }
}
