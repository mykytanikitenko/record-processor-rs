//! Memory-mapped queue integration tests against the real backing.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use record_processor_domain::client_id::ClientId;
    use record_processor_domain::input::{Transaction, TxId};
    use storage::mmap_queue::{AppendError, ReadAtError};
    use storage::{Append as _, MmapQueue, Position, ReadAt};

    #[rudzio::test]
    async fn append_batch_is_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = MmapQueue;
        let batch = [Transaction::Dispute {
            client: ClientId::from(1),
            tx: TxId::from(1),
        }];
        let appended = queue.append(&batch).await;
        anyhow::ensure!(
            appended == Err(AppendError::Unimplemented),
            "expected Unimplemented, got {appended:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn read_from_position_is_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = MmapQueue;
        let mut into = [Transaction::Eof; 4];
        let read = ReadAt::<Transaction>::read(&queue, Position::from(0), &mut into).await;
        anyhow::ensure!(
            read == Err(ReadAtError::Unimplemented),
            "expected Unimplemented, got {read:?}"
        );
        Ok(())
    }
}
