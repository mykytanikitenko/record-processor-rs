//! Consumer unit tests against a real in-memory queue and mocked state.

use rudzio::common::context::Suite;
use rudzio::runtime::compio::Runtime;

#[rudzio::suite([(runtime = Runtime::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use allocator_api2::alloc::Global;
    use core::cell::Cell;
    use core::convert::Infallible;
    use mockall::predicate::{always, eq};
    use record_processor_domain::client_id::ClientId;
    use record_processor_domain::input::{Amount, Transaction, TxId};
    use record_processor_domain::output::{AccountBalance, Balance, Parts as BalanceParts};
    use rust_decimal::Decimal;
    use service::Balances as _;
    use service::engine_state::MockState;
    use service::mocks::{
        MockClientsError, MockLoadClientError, MockLoadTxError, MockStoreClientError,
        MockStoreTxError,
    };
    use service::{
        BalanceStream, ConsumeError, Consumer as _, Engine, EngineParts, ServiceState,
        ServiceStateParts, TxKind, TxRecord, TxRecordParts, TxStatus,
    };
    use std::str::FromStr as _;
    use storage::MemoryQueue;
    use storage::memory_queue::Parts;
    use tokio_util::sync::CancellationToken;

    /// A transaction batch source that yields its rows once, then ends.
    #[derive(Debug)]
    struct TransactionSource {
        /// The rows yielded by [`TransactionSource::next_batch`].
        rows: Vec<Transaction>,
        /// Whether the rows have already been yielded.
        yielded: Cell<bool>,
    }

    impl service::Transactions for TransactionSource {
        type Error = Infallible;

        async fn next_batch<'stream>(
            &'stream mut self,
            _cancel: &CancellationToken,
        ) -> Result<Option<&'stream [Transaction]>, Self::Error> {
            if self.yielded.replace(true) {
                Ok(None)
            } else {
                Ok(Some(&self.rows))
            }
        }
    }

    /// A transaction source that reports cancellation as a typed error.
    #[derive(Debug)]
    struct CancelledSource;

    /// Failure reported by [`CancelledSource`] once its token is cancelled.
    #[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
    #[error("cancelled")]
    struct Cancelled;

    impl service::Transactions for CancelledSource {
        type Error = Cancelled;

        async fn next_batch<'stream>(
            &'stream mut self,
            cancel: &CancellationToken,
        ) -> Result<Option<&'stream [Transaction]>, Self::Error> {
            if cancel.is_cancelled() {
                Err(Cancelled)
            } else {
                Ok(None)
            }
        }
    }

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

    /// Build a deposit transaction record.
    fn deposit_record(client: u16, amount: &str, status: TxStatus) -> anyhow::Result<TxRecord> {
        Ok(TxRecord::from(TxRecordParts {
            kind: TxKind::Deposit,
            amount: Decimal::from_str(amount)?,
            client: ClientId::from(client),
            status,
        }))
    }

    /// Build a withdrawal transaction record.
    fn withdrawal_record(client: u16, amount: &str) -> anyhow::Result<TxRecord> {
        Ok(TxRecord::from(TxRecordParts {
            kind: TxKind::Withdrawal,
            amount: Decimal::from_str(amount)?,
            client: ClientId::from(client),
            status: TxStatus::Available,
        }))
    }

    /// Build a balance row.
    fn balance(
        client: u16,
        available: &str,
        held: &str,
        locked: bool,
    ) -> anyhow::Result<AccountBalance> {
        Ok(AccountBalance::try_from(BalanceParts {
            client: ClientId::from(client),
            available: Balance::from(Decimal::from_str(available)?),
            held: Balance::from(Decimal::from_str(held)?),
            locked,
        })?)
    }

    /// Drain `balances` fully, asserting the concatenated rows equal `expected`.
    async fn assert_balances<S>(
        mut balances: BalanceStream<S>,
        expected: &[AccountBalance],
    ) -> anyhow::Result<()>
    where
        S: service::State + core::fmt::Debug,
    {
        let mut got: Vec<AccountBalance> = Vec::new();
        loop {
            match balances.next_batch(&CancellationToken::new()).await {
                Ok(Some(rows)) => got.extend_from_slice(rows),
                Ok(None) => break,
                Err(error) => anyhow::bail!("unexpected balance error {error:?}"),
            }
        }
        anyhow::ensure!(
            got.as_slice() == expected,
            "unexpected balances {got:?}, expected {expected:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn deposit_credits_new_account(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let stored_record = deposit_record(1, "1.5", TxStatus::Available)?;
        let stored_client = service_state("1.5", "0.0", false)?;
        let mut source = TransactionSource {
            rows: vec![deposit(1, 1, "1.5")?],
            yielded: Cell::new(false),
        };
        let queue = MemoryQueue::from(Parts { alloc: Global });
        let mut state = MockState::new();
        let _load_tx = state
            .expect_load_tx()
            .with(eq(TxId::from(1)), always())
            .times(1)
            .returning(|_tx, _cancel| -> Result<Option<TxRecord>, MockLoadTxError> { Ok(None) });
        let _store_tx = state
            .expect_store_tx()
            .with(eq(TxId::from(1)), eq(stored_record), always())
            .times(1)
            .returning(|_tx, _state, _cancel| -> Result<(), MockStoreTxError> { Ok(()) });
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
        let _next_client = state.expect_next_client().times(2).returning(
            move |after: Option<ClientId>,
                  _cancel: &CancellationToken|
                  -> Result<Option<(ClientId, ServiceState)>, MockClientsError> {
                if after.is_none() {
                    Ok(Some((ClientId::from(1), stored_client)))
                } else {
                    Ok(None)
                }
            },
        );
        let engine = Engine::from(EngineParts { queue, state });
        let balances = engine
            .consume(&CancellationToken::new(), &mut source)
            .await?;
        assert_balances(balances, &[balance(1, "1.5", "0.0", false)?]).await
    }

    #[rudzio::test]
    async fn withdrawal_debits_sufficient_funds(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let stored_record = withdrawal_record(1, "4.0")?;
        let loaded = service_state("10.0", "0.0", false)?;
        let stored = service_state("6.0", "0.0", false)?;
        let mut source = TransactionSource {
            rows: vec![withdrawal(1, 1, "4.0")?],
            yielded: Cell::new(false),
        };
        let queue = MemoryQueue::from(Parts { alloc: Global });
        let mut state = MockState::new();
        let _load_tx = state
            .expect_load_tx()
            .with(eq(TxId::from(1)), always())
            .times(1)
            .returning(|_tx, _cancel| -> Result<Option<TxRecord>, MockLoadTxError> { Ok(None) });
        let _store_tx = state
            .expect_store_tx()
            .with(eq(TxId::from(1)), eq(stored_record), always())
            .times(1)
            .returning(|_tx, _state, _cancel| -> Result<(), MockStoreTxError> { Ok(()) });
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
        let _next_client = state.expect_next_client().times(2).returning(
            move |after: Option<ClientId>,
                  _cancel: &CancellationToken|
                  -> Result<Option<(ClientId, ServiceState)>, MockClientsError> {
                if after.is_none() {
                    Ok(Some((ClientId::from(1), stored)))
                } else {
                    Ok(None)
                }
            },
        );
        let engine = Engine::from(EngineParts { queue, state });
        let balances = engine
            .consume(&CancellationToken::new(), &mut source)
            .await?;
        assert_balances(balances, &[balance(1, "6.0", "0.0", false)?]).await
    }

    #[rudzio::test]
    async fn withdrawal_insufficient_is_ignored(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let stored_record = withdrawal_record(1, "20.0")?;
        let loaded = service_state("10.0", "0.0", false)?;
        let mut source = TransactionSource {
            rows: vec![withdrawal(1, 1, "20.0")?],
            yielded: Cell::new(false),
        };
        let queue = MemoryQueue::from(Parts { alloc: Global });
        let mut state = MockState::new();
        let _load_tx = state
            .expect_load_tx()
            .with(eq(TxId::from(1)), always())
            .times(1)
            .returning(|_tx, _cancel| -> Result<Option<TxRecord>, MockLoadTxError> { Ok(None) });
        let _store_tx = state
            .expect_store_tx()
            .with(eq(TxId::from(1)), eq(stored_record), always())
            .times(1)
            .returning(|_tx, _state, _cancel| -> Result<(), MockStoreTxError> { Ok(()) });
        let _load_client = state
            .expect_load_client()
            .with(eq(ClientId::from(1)), always())
            .times(1)
            .returning(
                move |_client, _cancel| -> Result<Option<ServiceState>, MockLoadClientError> {
                    Ok(Some(loaded))
                },
            );
        let _next_client = state.expect_next_client().times(2).returning(
            move |after: Option<ClientId>,
                  _cancel: &CancellationToken|
                  -> Result<Option<(ClientId, ServiceState)>, MockClientsError> {
                if after.is_none() {
                    Ok(Some((ClientId::from(1), loaded)))
                } else {
                    Ok(None)
                }
            },
        );
        let engine = Engine::from(EngineParts { queue, state });
        let balances = engine
            .consume(&CancellationToken::new(), &mut source)
            .await?;
        assert_balances(balances, &[balance(1, "10.0", "0.0", false)?]).await
    }

    #[rudzio::test]
    async fn withdrawal_new_client_creates_zeroed_record(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let stored_record = withdrawal_record(1, "5.0")?;
        let zeroed = service_state("0.0", "0.0", false)?;
        let mut source = TransactionSource {
            rows: vec![withdrawal(1, 1, "5.0")?],
            yielded: Cell::new(false),
        };
        let queue = MemoryQueue::from(Parts { alloc: Global });
        let mut state = MockState::new();
        let _load_tx = state
            .expect_load_tx()
            .with(eq(TxId::from(1)), always())
            .times(1)
            .returning(|_tx, _cancel| -> Result<Option<TxRecord>, MockLoadTxError> { Ok(None) });
        let _store_tx = state
            .expect_store_tx()
            .with(eq(TxId::from(1)), eq(stored_record), always())
            .times(1)
            .returning(|_tx, _state, _cancel| -> Result<(), MockStoreTxError> { Ok(()) });
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
            .with(eq(ClientId::from(1)), eq(zeroed), always())
            .times(1)
            .returning(|_client, _state, _cancel| -> Result<(), MockStoreClientError> { Ok(()) });
        let _next_client = state.expect_next_client().times(2).returning(
            move |after: Option<ClientId>,
                  _cancel: &CancellationToken|
                  -> Result<Option<(ClientId, ServiceState)>, MockClientsError> {
                if after.is_none() {
                    Ok(Some((ClientId::from(1), zeroed)))
                } else {
                    Ok(None)
                }
            },
        );
        let engine = Engine::from(EngineParts { queue, state });
        let balances = engine
            .consume(&CancellationToken::new(), &mut source)
            .await?;
        assert_balances(balances, &[balance(1, "0.0", "0.0", false)?]).await
    }

    #[rudzio::test]
    async fn dispute_moves_available_to_held(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let available = deposit_record(1, "10.0", TxStatus::Available)?;
        let disputed = deposit_record(1, "10.0", TxStatus::Disputed)?;
        let loaded = service_state("10.0", "0.0", false)?;
        let stored = service_state("0.0", "10.0", false)?;
        let mut source = TransactionSource {
            rows: vec![dispute(1, 1)],
            yielded: Cell::new(false),
        };
        let queue = MemoryQueue::from(Parts { alloc: Global });
        let mut state = MockState::new();
        let _load_tx = state
            .expect_load_tx()
            .with(eq(TxId::from(1)), always())
            .times(1)
            .returning(
                move |_tx, _cancel| -> Result<Option<TxRecord>, MockLoadTxError> {
                    Ok(Some(available))
                },
            );
        let _store_tx = state
            .expect_store_tx()
            .with(eq(TxId::from(1)), eq(disputed), always())
            .times(1)
            .returning(|_tx, _state, _cancel| -> Result<(), MockStoreTxError> { Ok(()) });
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
        let _next_client = state.expect_next_client().times(2).returning(
            move |after: Option<ClientId>,
                  _cancel: &CancellationToken|
                  -> Result<Option<(ClientId, ServiceState)>, MockClientsError> {
                if after.is_none() {
                    Ok(Some((ClientId::from(1), stored)))
                } else {
                    Ok(None)
                }
            },
        );
        let engine = Engine::from(EngineParts { queue, state });
        let balances = engine
            .consume(&CancellationToken::new(), &mut source)
            .await?;
        assert_balances(balances, &[balance(1, "0.0", "10.0", false)?]).await
    }

    #[rudzio::test]
    async fn resolve_releases_held_to_available(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let disputed = deposit_record(1, "10.0", TxStatus::Disputed)?;
        let available = deposit_record(1, "10.0", TxStatus::Available)?;
        let loaded = service_state("0.0", "10.0", false)?;
        let stored = service_state("10.0", "0.0", false)?;
        let mut source = TransactionSource {
            rows: vec![resolve(1, 1)],
            yielded: Cell::new(false),
        };
        let queue = MemoryQueue::from(Parts { alloc: Global });
        let mut state = MockState::new();
        let _load_tx = state
            .expect_load_tx()
            .with(eq(TxId::from(1)), always())
            .times(1)
            .returning(
                move |_tx, _cancel| -> Result<Option<TxRecord>, MockLoadTxError> {
                    Ok(Some(disputed))
                },
            );
        let _store_tx = state
            .expect_store_tx()
            .with(eq(TxId::from(1)), eq(available), always())
            .times(1)
            .returning(|_tx, _state, _cancel| -> Result<(), MockStoreTxError> { Ok(()) });
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
        let _next_client = state.expect_next_client().times(2).returning(
            move |after: Option<ClientId>,
                  _cancel: &CancellationToken|
                  -> Result<Option<(ClientId, ServiceState)>, MockClientsError> {
                if after.is_none() {
                    Ok(Some((ClientId::from(1), stored)))
                } else {
                    Ok(None)
                }
            },
        );
        let engine = Engine::from(EngineParts { queue, state });
        let balances = engine
            .consume(&CancellationToken::new(), &mut source)
            .await?;
        assert_balances(balances, &[balance(1, "10.0", "0.0", false)?]).await
    }

    #[rudzio::test]
    async fn chargeback_drains_held_and_locks(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let disputed = deposit_record(1, "10.0", TxStatus::Disputed)?;
        let charged_back = deposit_record(1, "10.0", TxStatus::ChargedBack)?;
        let loaded = service_state("0.0", "10.0", false)?;
        let stored = service_state("0.0", "0.0", true)?;
        let mut source = TransactionSource {
            rows: vec![chargeback(1, 1)],
            yielded: Cell::new(false),
        };
        let queue = MemoryQueue::from(Parts { alloc: Global });
        let mut state = MockState::new();
        let _load_tx = state
            .expect_load_tx()
            .with(eq(TxId::from(1)), always())
            .times(1)
            .returning(
                move |_tx, _cancel| -> Result<Option<TxRecord>, MockLoadTxError> {
                    Ok(Some(disputed))
                },
            );
        let _store_tx = state
            .expect_store_tx()
            .with(eq(TxId::from(1)), eq(charged_back), always())
            .times(1)
            .returning(|_tx, _state, _cancel| -> Result<(), MockStoreTxError> { Ok(()) });
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
        let _next_client = state.expect_next_client().times(2).returning(
            move |after: Option<ClientId>,
                  _cancel: &CancellationToken|
                  -> Result<Option<(ClientId, ServiceState)>, MockClientsError> {
                if after.is_none() {
                    Ok(Some((ClientId::from(1), stored)))
                } else {
                    Ok(None)
                }
            },
        );
        let engine = Engine::from(EngineParts { queue, state });
        let balances = engine
            .consume(&CancellationToken::new(), &mut source)
            .await?;
        assert_balances(balances, &[balance(1, "0.0", "0.0", true)?]).await
    }

    #[rudzio::test]
    async fn dispute_unknown_tx_is_ignored(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut source = TransactionSource {
            rows: vec![dispute(1, 999)],
            yielded: Cell::new(false),
        };
        let queue = MemoryQueue::from(Parts { alloc: Global });
        let mut state = MockState::new();
        let _load_tx = state
            .expect_load_tx()
            .with(eq(TxId::from(999)), always())
            .times(1)
            .returning(|_tx, _cancel| -> Result<Option<TxRecord>, MockLoadTxError> { Ok(None) });
        let _next_client = state.expect_next_client().times(1).returning(
            |_after: Option<ClientId>,
             _cancel: &CancellationToken|
             -> Result<Option<(ClientId, ServiceState)>, MockClientsError> { Ok(None) },
        );
        let engine = Engine::from(EngineParts { queue, state });
        let balances = engine
            .consume(&CancellationToken::new(), &mut source)
            .await?;
        assert_balances(balances, &[]).await
    }

    #[rudzio::test]
    async fn dispute_on_withdrawal_is_ignored(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let recorded = withdrawal_record(1, "5.0")?;
        let mut source = TransactionSource {
            rows: vec![dispute(1, 2)],
            yielded: Cell::new(false),
        };
        let queue = MemoryQueue::from(Parts { alloc: Global });
        let mut state = MockState::new();
        let _load_tx = state
            .expect_load_tx()
            .with(eq(TxId::from(2)), always())
            .times(1)
            .returning(
                move |_tx, _cancel| -> Result<Option<TxRecord>, MockLoadTxError> {
                    Ok(Some(recorded))
                },
            );
        let _next_client = state.expect_next_client().times(1).returning(
            |_after: Option<ClientId>,
             _cancel: &CancellationToken|
             -> Result<Option<(ClientId, ServiceState)>, MockClientsError> { Ok(None) },
        );
        let engine = Engine::from(EngineParts { queue, state });
        let balances = engine
            .consume(&CancellationToken::new(), &mut source)
            .await?;
        assert_balances(balances, &[]).await
    }

    #[rudzio::test]
    async fn cancellation_ends_reading_the_stream(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = MemoryQueue::from(Parts { alloc: Global });
        let state = MockState::new();
        let engine = Engine::from(EngineParts { queue, state });
        let cancel = CancellationToken::new();
        cancel.cancel();
        let mut source = CancelledSource;
        let consumed = engine.consume(&cancel, &mut source).await;
        anyhow::ensure!(
            matches!(consumed, Err(ConsumeError::Stream(Cancelled))),
            "unexpected consume {consumed:?}"
        );
        Ok(())
    }
}
