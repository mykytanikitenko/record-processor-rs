//! Engine stage tests driving each stage against mocked storage.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use record_processor_domain::input::Transaction;
    use service::{
        AggregateError, BalanceStream as _, Engine, IngestError, NextBalanceError,
        TransactionStream,
    };
    use storage::{Position, Span};
    use tokio_util::sync::CancellationToken;
    use tokio_util::task::TaskTracker;

    use crate::mocks::{MockClientState, MockTransactionQueue, QueueFailure};

    /// An always-empty transaction source.
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

    #[rudzio::test]
    async fn ingest_against_mock_queue_is_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut queue = MockTransactionQueue::new();
        let _never_append = queue.expect_append().never();
        let kv = MockClientState::new();
        let engine = Engine {
            kv: &kv,
            queue: &queue,
        };
        let token = CancellationToken::new();
        let mut batch = [Transaction::Eof; 4];
        let ingested = engine
            .ingest(&mut EmptyTransactions, &mut batch, &token)
            .await;
        anyhow::ensure!(
            ingested == Err(IngestError::<core::convert::Infallible, QueueFailure>::Unimplemented),
            "expected Unimplemented, got {ingested:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn aggregate_against_mock_stores_is_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut queue = MockTransactionQueue::new();
        let _never_read = queue.expect_read().never();
        let mut kv = MockClientState::new();
        let _never_update = kv.expect_update().never();
        let engine = Engine {
            kv: &kv,
            queue: &queue,
        };
        let token = CancellationToken::new();
        let tracker = TaskTracker::new();
        let mut batch = [Transaction::Eof; 4];
        let ingested = Span {
            end: Position::from(0),
            start: Position::from(0),
        };
        let applied = engine
            .aggregate(Position::from(0), ingested, &mut batch, &tracker, &token)
            .await;
        anyhow::ensure!(
            applied == Err(AggregateError::Unimplemented),
            "expected Unimplemented, got {applied:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn balances_against_mock_state_are_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = MockTransactionQueue::new();
        let mut kv = MockClientState::new();
        let _never_next_entry = kv.expect_next_entry().never();
        let engine = Engine {
            kv: &kv,
            queue: &queue,
        };
        let token = CancellationToken::new();
        let mut balances = engine.balances();
        let next = balances.next(&token).await;
        anyhow::ensure!(
            next == Err(NextBalanceError::Unimplemented),
            "expected Unimplemented, got {next:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn cancelled_ingest_yields_cancelled(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = MockTransactionQueue::new();
        let kv = MockClientState::new();
        let engine = Engine {
            kv: &kv,
            queue: &queue,
        };
        let token = CancellationToken::new();
        token.cancel();
        let mut batch = [Transaction::Eof; 4];
        let ingested = engine
            .ingest(&mut EmptyTransactions, &mut batch, &token)
            .await;
        anyhow::ensure!(
            ingested == Err(IngestError::Cancelled),
            "expected Cancelled, got {ingested:?}"
        );
        Ok(())
    }
}
