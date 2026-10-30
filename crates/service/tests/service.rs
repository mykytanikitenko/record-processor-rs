//! Service unit tests: the engine driven against mockall mocks of every
//! storage trait, proving each contract is mockable.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use record_processor_domain::client_id::ClientId;
    use record_processor_domain::input::{Transaction, TxId};
    use record_processor_domain::output::AccountBalance;
    use service::{
        BalanceStream as _, Engine, EngineError, RealBalances, RealBalancesError, ServeError,
        Service as _, TransactionStream,
    };
    use storage::{
        IdRange, IndexInsertError, Insertion, KvEntries, QueueAppendError, QueueReadError,
        SignalNotifyError, SignalWaitError, StateReadError, StateWriteError,
    };
    use tokio_util::sync::CancellationToken;

    mockall::mock! {
        Queue {}
        impl storage::EventQueue for Queue {
            type AppendBatchError = QueueAppendError;
            type ReadBatchError = QueueReadError;
            async fn append_batch(
                &mut self,
                events: &[Transaction],
                cancel: &CancellationToken,
            ) -> Result<IdRange, QueueAppendError>;
            async fn read_batch(
                &mut self,
                from: u64,
                events: &mut [Transaction],
                cancel: &CancellationToken,
            ) -> Result<usize, QueueReadError>;
        }
    }

    mockall::mock! {
        Index {}
        impl storage::DedupIndex for Index {
            type InsertIdError = IndexInsertError;
            async fn insert(
                &mut self,
                tx: TxId,
                cancel: &CancellationToken,
            ) -> Result<Insertion, IndexInsertError>;
        }
    }

    mockall::mock! {
        Eof {}
        impl storage::EndOfInput for Eof {
            type NotifyEndError = SignalNotifyError;
            type WaitEndError = SignalWaitError;
            async fn notify(&mut self, end: u64, cancel: &CancellationToken)
                -> Result<(), SignalNotifyError>;
            async fn wait(&mut self, cancel: &CancellationToken) -> Result<u64, SignalWaitError>;
        }
    }

    mockall::mock! {
        State {}
        impl storage::KvState for State {
            type ReadStateError = StateReadError;
            type WriteStateError = StateWriteError;
            fn entries(&mut self) -> KvEntries;
            async fn read(
                &mut self,
                client: ClientId,
                cancel: &CancellationToken,
            ) -> Result<Option<AccountBalance>, StateReadError>;
            async fn write(
                &mut self,
                client: ClientId,
                balance: &AccountBalance,
                cancel: &CancellationToken,
            ) -> Result<(), StateWriteError>;
        }
    }

    /// An always-empty transaction source feeding the engine.
    struct EmptyTransactions;

    impl TransactionStream for EmptyTransactions {
        type Error = core::convert::Infallible;

        async fn next(
            &mut self,
            _cancel: &CancellationToken,
        ) -> Result<Option<Transaction>, Self::Error> {
            Ok(None)
        }
    }

    /// Engine over four expectation-free mocks; a test configures the one it
    /// targets before serving.
    fn engine() -> Engine<MockQueue, MockIndex, MockEof, MockState> {
        Engine {
            index: MockIndex::new(),
            eof: MockEof::new(),
            queue: MockQueue::new(),
            state: MockState::new(),
        }
    }

    /// Serve an empty input and require the engine's typed `Unimplemented`.
    async fn ensure_unimplemented(
        engine: Engine<MockQueue, MockIndex, MockEof, MockState>,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let result = engine.serve(EmptyTransactions, &token).await;
        anyhow::ensure!(
            result == Err(ServeError::Impl(EngineError::Unimplemented)),
            "expected Unimplemented, got {result:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn serve_against_mock_event_queue(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut engine = engine();
        let _append_batch = engine.queue.expect_append_batch().never();
        let _read_batch = engine.queue.expect_read_batch().never();
        ensure_unimplemented(engine).await
    }

    #[rudzio::test]
    async fn serve_against_mock_dedup_index(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut engine = engine();
        let _insert = engine.index.expect_insert().never();
        ensure_unimplemented(engine).await
    }

    #[rudzio::test]
    async fn serve_against_mock_end_of_input(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut engine = engine();
        let _notify = engine.eof.expect_notify().never();
        let _wait = engine.eof.expect_wait().never();
        ensure_unimplemented(engine).await
    }

    #[rudzio::test]
    async fn serve_against_mock_kv_state(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut engine = engine();
        let _entries = engine.state.expect_entries().never();
        let _read = engine.state.expect_read().never();
        let _write = engine.state.expect_write().never();
        ensure_unimplemented(engine).await
    }

    #[rudzio::test]
    async fn real_balances_are_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut balances = RealBalances;
        let item = balances.next(&token).await;
        anyhow::ensure!(
            item == Err(RealBalancesError::Unimplemented),
            "expected Unimplemented, got {item:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn real_balances_yield_cancelled(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        token.cancel();
        let mut balances = RealBalances;
        let item = balances.next(&token).await;
        anyhow::ensure!(
            item == Err(RealBalancesError::Cancelled),
            "expected Cancelled, got {item:?}"
        );
        Ok(())
    }
}
