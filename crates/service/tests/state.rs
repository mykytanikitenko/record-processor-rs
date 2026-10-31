//! State unit tests against mocked key-value stores.

use rudzio::common::context::Suite;
use rudzio::runtime::compio::Runtime;

#[rudzio::suite([(runtime = Runtime::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use mockall::predicate::{always, eq};
    use record_processor_domain::client_id::ClientId;
    use record_processor_domain::input::TxId;
    use service::engine_state::{KvState, LoadClientError, LoadTxError, StoreClientError};
    use service::{
        ServiceState, ServiceStateParts, State as _, TxKind, TxRecord, TxRecordParts, TxStatus,
    };
    use storage::kv::MockKv;
    use storage::mocks::{MockLoadError, MockStoreError};
    use tokio_util::sync::CancellationToken;

    /// The client store double.
    type Clients = MockKv<ClientId, ServiceState>;
    /// The transaction store double.
    type Records = MockKv<TxId, TxRecord>;

    /// Build a service state to store.
    fn service_state(available: u64, held: u64, locked: bool) -> ServiceState {
        ServiceState::from(ServiceStateParts {
            available: rust_decimal::Decimal::from(available),
            held: rust_decimal::Decimal::from(held),
            locked,
        })
    }

    /// Build a transaction record to store.
    fn tx_record(client: u16, status: TxStatus) -> TxRecord {
        TxRecord::from(TxRecordParts {
            kind: TxKind::Deposit,
            amount: rust_decimal::Decimal::ONE,
            client: ClientId::from(client),
            status,
        })
    }

    #[rudzio::test]
    async fn load_client_propagates_load_failure(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut clients = Clients::new();
        let _expectation = clients.expect_load().times(1).returning(
            |_key, _cancel| -> Result<Option<ServiceState>, MockLoadError> { Err(MockLoadError) },
        );
        let records = Records::new();
        let state = KvState {
            clients: &clients,
            transactions: &records,
        };
        let loaded = state
            .load_client(ClientId::from(7), &CancellationToken::new())
            .await;
        anyhow::ensure!(
            matches!(loaded, Err(LoadClientError::Load(MockLoadError))),
            "unexpected load {loaded:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn load_client_returns_stored_state(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let stored = service_state(1, 2, true);
        let mut clients = Clients::new();
        let _expectation = clients
            .expect_load()
            .with(eq(ClientId::from(7)), always())
            .times(1)
            .returning(
                move |_key, _cancel| -> Result<Option<ServiceState>, MockLoadError> {
                    Ok(Some(stored))
                },
            );
        let records = Records::new();
        let state = KvState {
            clients: &clients,
            transactions: &records,
        };
        let loaded = state
            .load_client(ClientId::from(7), &CancellationToken::new())
            .await;
        anyhow::ensure!(
            matches!(loaded, Ok(Some(found)) if found == stored),
            "unexpected load {loaded:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn store_client_propagates_store_failure(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let stored = service_state(1, 0, false);
        let mut clients = Clients::new();
        let _expectation = clients.expect_store().times(1).returning(
            |_key, _value, _cancel| -> Result<(), MockStoreError> { Err(MockStoreError) },
        );
        let records = Records::new();
        let state = KvState {
            clients: &clients,
            transactions: &records,
        };
        let stored_state = state
            .store_client(ClientId::from(7), stored, &CancellationToken::new())
            .await;
        anyhow::ensure!(
            matches!(stored_state, Err(StoreClientError::Store(MockStoreError))),
            "unexpected store {stored_state:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn store_client_stores_client_state(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let stored = service_state(1, 2, true);
        let mut clients = Clients::new();
        let _expectation = clients
            .expect_store()
            .with(eq(ClientId::from(7)), eq(stored), always())
            .times(1)
            .returning(|_key, _value, _cancel| -> Result<(), MockStoreError> { Ok(()) });
        let records = Records::new();
        let state = KvState {
            clients: &clients,
            transactions: &records,
        };
        let stored_state = state
            .store_client(ClientId::from(7), stored, &CancellationToken::new())
            .await;
        anyhow::ensure!(
            matches!(stored_state, Ok(())),
            "unexpected store {stored_state:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn load_tx_returns_stored_state(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let stored = tx_record(3, TxStatus::Disputed);
        let clients = Clients::new();
        let mut records = Records::new();
        let _expectation = records
            .expect_load()
            .with(eq(TxId::from(9)), always())
            .times(1)
            .returning(
                move |_key, _cancel| -> Result<Option<TxRecord>, MockLoadError> {
                    Ok(Some(stored))
                },
            );
        let state = KvState {
            clients: &clients,
            transactions: &records,
        };
        let loaded = state
            .load_tx(TxId::from(9), &CancellationToken::new())
            .await;
        anyhow::ensure!(
            matches!(loaded, Ok(Some(found)) if found == stored),
            "unexpected load {loaded:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn load_tx_propagates_load_failure(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let clients = Clients::new();
        let mut records = Records::new();
        let _expectation = records.expect_load().times(1).returning(
            |_key, _cancel| -> Result<Option<TxRecord>, MockLoadError> { Err(MockLoadError) },
        );
        let state = KvState {
            clients: &clients,
            transactions: &records,
        };
        let loaded = state
            .load_tx(TxId::from(9), &CancellationToken::new())
            .await;
        anyhow::ensure!(
            matches!(loaded, Err(LoadTxError::Load(MockLoadError))),
            "unexpected load {loaded:?}"
        );
        Ok(())
    }
}
