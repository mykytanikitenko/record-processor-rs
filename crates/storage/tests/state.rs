//! State unit tests against mocked key-value stores.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use mockall::predicate::eq;
    use record_processor_domain::client_id::ClientId;
    use record_processor_domain::output::{AccountBalance, Balance, Parts};
    use rust_decimal::Decimal;
    use storage::kv::MockKv;
    use storage::mocks::{MockLoadError, MockStoreError};
    use storage::state::{
        CommitOffsetError, CurrentOffsetError, StateLoadError, StateUpdateError,
    };
    use storage::{ClientState, CommittedOffset, KvState, Offset, State as _};
    use tokio_util::sync::CancellationToken;

    /// The offset store double.
    type Offsets = MockKv<CommittedOffset, Offset>;
    /// The client store double.
    type Clients = MockKv<ClientId, ClientState>;

    /// Build a client state to store.
    fn client_state() -> anyhow::Result<ClientState> {
        Ok(ClientState::from(AccountBalance::try_from(Parts {
            client: ClientId::from(7),
            available: Balance::from(Decimal::ONE),
            held: Balance::from(Decimal::ZERO),
            locked: false,
        })?))
    }

    #[rudzio::test]
    async fn current_offset_loads_committed_key(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut offsets = Offsets::new();
        let _expectation = offsets
            .expect_load()
            .with(eq(CommittedOffset), mockall::predicate::always())
            .times(1)
            .returning(|_key, _cancel| Ok(Some(Offset::from(42))));
        let clients = Clients::new();
        let state = KvState {
            offsets: &offsets,
            clients: &clients,
        };
        let current = state.current_offset(&CancellationToken::new()).await;
        anyhow::ensure!(
            matches!(current, Ok(Some(offset)) if offset == Offset::from(42)),
            "unexpected current offset {current:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn current_offset_propagates_load_failure(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut offsets = Offsets::new();
        let _expectation = offsets
            .expect_load()
            .times(1)
            .returning(|_key, _cancel| Err(MockLoadError));
        let clients = Clients::new();
        let state = KvState {
            offsets: &offsets,
            clients: &clients,
        };
        let current = state.current_offset(&CancellationToken::new()).await;
        anyhow::ensure!(
            matches!(current, Err(CurrentOffsetError::Load(MockLoadError))),
            "unexpected current offset {current:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn commit_offset_stores_committed_key(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut offsets = Offsets::new();
        let _expectation = offsets
            .expect_store()
            .with(
                eq(CommittedOffset),
                eq(Offset::from(9)),
                mockall::predicate::always(),
            )
            .times(1)
            .returning(|_key, _value, _cancel| Ok(()));
        let clients = Clients::new();
        let state = KvState {
            offsets: &offsets,
            clients: &clients,
        };
        let committed = state
            .commit_offset(Offset::from(9), &CancellationToken::new())
            .await;
        anyhow::ensure!(matches!(committed, Ok(())), "unexpected commit {committed:?}");
        Ok(())
    }

    #[rudzio::test]
    async fn commit_offset_propagates_store_failure(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut offsets = Offsets::new();
        let _expectation = offsets
            .expect_store()
            .times(1)
            .returning(|_key, _value, _cancel| Err(MockStoreError));
        let clients = Clients::new();
        let state = KvState {
            offsets: &offsets,
            clients: &clients,
        };
        let committed = state
            .commit_offset(Offset::from(9), &CancellationToken::new())
            .await;
        anyhow::ensure!(
            matches!(committed, Err(CommitOffsetError::Store(MockStoreError))),
            "unexpected commit {committed:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn update_stores_client_state(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let stored = client_state()?;
        let offsets = Offsets::new();
        let mut clients = Clients::new();
        let _expectation = clients
            .expect_store()
            .with(
                eq(ClientId::from(7)),
                eq(stored),
                mockall::predicate::always(),
            )
            .times(1)
            .returning(|_key, _value, _cancel| Ok(()));
        let state = KvState {
            offsets: &offsets,
            clients: &clients,
        };
        let updated = state
            .update(ClientId::from(7), stored, &CancellationToken::new())
            .await;
        anyhow::ensure!(matches!(updated, Ok(())), "unexpected update {updated:?}");
        Ok(())
    }

    #[rudzio::test]
    async fn update_propagates_store_failure(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let stored = client_state()?;
        let offsets = Offsets::new();
        let mut clients = Clients::new();
        let _expectation = clients
            .expect_store()
            .times(1)
            .returning(|_key, _value, _cancel| Err(MockStoreError));
        let state = KvState {
            offsets: &offsets,
            clients: &clients,
        };
        let updated = state
            .update(ClientId::from(7), stored, &CancellationToken::new())
            .await;
        anyhow::ensure!(
            matches!(updated, Err(StateUpdateError::Store(MockStoreError))),
            "unexpected update {updated:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn load_returns_client_state(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let stored = client_state()?;
        let offsets = Offsets::new();
        let mut clients = Clients::new();
        let _expectation = clients
            .expect_load()
            .with(eq(ClientId::from(7)), mockall::predicate::always())
            .times(1)
            .returning(move |_key, _cancel| Ok(Some(stored)));
        let state = KvState {
            offsets: &offsets,
            clients: &clients,
        };
        let loaded = state
            .load(ClientId::from(7), &CancellationToken::new())
            .await;
        anyhow::ensure!(
            matches!(loaded, Ok(Some(found)) if found == stored),
            "unexpected load {loaded:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn load_propagates_load_failure(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let offsets = Offsets::new();
        let mut clients = Clients::new();
        let _expectation = clients
            .expect_load()
            .times(1)
            .returning(|_key, _cancel| Err(MockLoadError));
        let state = KvState {
            offsets: &offsets,
            clients: &clients,
        };
        let loaded = state
            .load(ClientId::from(7), &CancellationToken::new())
            .await;
        anyhow::ensure!(
            matches!(loaded, Err(StateLoadError::Load(MockLoadError))),
            "unexpected load {loaded:?}"
        );
        Ok(())
    }
}
