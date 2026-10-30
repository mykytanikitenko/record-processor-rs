//! Storage mock tests: each mock is driven through its configured behaviour.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use record_processor_domain::input::{Transaction, TxId};
    use storage::{
        DedupIndex as _, EndOfInput as _, EndOfInputError, EventQueue as _, IndexError, KvEntries,
        KvState as _, MockDedupIndex, MockEndOfInput, MockEventQueue, MockKvState, NextEntryError,
        QueueError, StateError,
    };
    use tokio_util::sync::CancellationToken;

    #[rudzio::test]
    async fn mock_event_queue_appends(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let mut mock = MockEventQueue::new();
        let _expectation = mock
            .expect_append()
            .returning(|_| -> Result<u32, QueueError> { Ok(7_u32) });
        let id = mock.append(&Transaction::Eof).await?;
        anyhow::ensure!(id == 7_u32, "expected id 7, got {id}");
        Ok(())
    }

    #[rudzio::test]
    async fn mock_event_queue_reads_batch(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut mock = MockEventQueue::new();
        let _expectation = mock
            .expect_read_batch()
            .returning(|buf| -> Result<usize, QueueError> { Ok(buf.len()) });
        let mut buf = [0_u8; 8];
        let read = mock.read_batch(&mut buf).await?;
        anyhow::ensure!(read == 8, "expected 8, got {read}");
        Ok(())
    }

    #[rudzio::test]
    async fn mock_dedup_index_inserts(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let mut mock = MockDedupIndex::new();
        let _expectation = mock
            .expect_insert()
            .returning(|_| -> Result<(), IndexError> { Ok(()) });
        mock.insert(&TxId::from(1)).await?;
        Ok(())
    }

    #[rudzio::test]
    async fn mock_dedup_index_rejects_duplicate(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut mock = MockDedupIndex::new();
        let _expectation = mock
            .expect_insert()
            .returning(|tx| -> Result<(), IndexError> { Err(IndexError::Duplicate(*tx)) });
        let result = mock.insert(&TxId::from(1)).await;
        anyhow::ensure!(
            result == Err(IndexError::Duplicate(TxId::from(1))),
            "expected Duplicate, got {result:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn mock_kv_state_updates(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let mut mock = MockKvState::new();
        let _expectation = mock
            .expect_update()
            .returning(|_, _| -> Result<(), StateError> { Ok(()) });
        let balance = record_processor_domain::output::AccountBalance::try_from(
            record_processor_domain::output::Parts {
                client: record_processor_domain::client_id::ClientId::from(1),
                available: record_processor_domain::output::Balance::from(
                    rust_decimal::Decimal::ONE,
                ),
                held: record_processor_domain::output::Balance::from(rust_decimal::Decimal::ZERO),
                locked: false,
            },
        )?;
        mock.update(
            record_processor_domain::client_id::ClientId::from(1),
            &balance,
        )
        .await?;
        Ok(())
    }

    #[rudzio::test]
    async fn mock_kv_state_entries(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut mock = MockKvState::new();
        let _expectation = mock.expect_entries().returning(|| KvEntries::from(()));
        let mut entries = mock.entries();
        let first = entries.next(&token).await;
        anyhow::ensure!(
            first == Err(NextEntryError::Unimplemented),
            "expected Unimplemented, got {first:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn mock_end_of_input_notifies(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut mock = MockEndOfInput::new();
        let _expectation = mock
            .expect_notify()
            .returning(|| -> Result<(), EndOfInputError> { Ok(()) });
        mock.notify().await?;
        Ok(())
    }

    #[rudzio::test]
    async fn mock_end_of_input_waits(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let mut mock = MockEndOfInput::new();
        let _expectation = mock
            .expect_wait()
            .returning(|| -> Result<(), EndOfInputError> { Ok(()) });
        mock.wait().await?;
        Ok(())
    }
}
