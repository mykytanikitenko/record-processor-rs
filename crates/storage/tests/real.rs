//! Production storage stub tests: every method returns `Unimplemented`.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use record_processor_domain::input::{Transaction, TxId};
    use storage::{
        DedupIndex as _, EndOfInput as _, EndOfInputError, EventQueue as _, IndexError,
        KvState as _, NextEntryError, QueueError, StateError, Storage,
    };
    use tokio_util::sync::CancellationToken;

    #[rudzio::test]
    async fn event_queue_is_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut storage = Storage;
        let appended = storage.append(&Transaction::Eof).await;
        anyhow::ensure!(
            appended == Err(QueueError::Unimplemented),
            "expected Unimplemented, got {appended:?}"
        );
        let mut buf = [0_u8; 16];
        let read = storage.read_batch(&mut buf).await;
        anyhow::ensure!(
            read == Err(QueueError::Unimplemented),
            "expected Unimplemented, got {read:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn dedup_index_is_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut storage = Storage;
        let inserted = storage.insert(&TxId::from(1)).await;
        anyhow::ensure!(
            inserted == Err(IndexError::Unimplemented),
            "expected Unimplemented, got {inserted:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn kv_state_is_unimplemented(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut storage = Storage;
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
        let updated = storage
            .update(
                record_processor_domain::client_id::ClientId::from(1),
                &balance,
            )
            .await;
        anyhow::ensure!(
            updated == Err(StateError::Unimplemented),
            "expected Unimplemented, got {updated:?}"
        );
        let mut entries = storage.entries();
        let first = entries.next(&token).await;
        anyhow::ensure!(
            first == Err(NextEntryError::Unimplemented),
            "expected Unimplemented, got {first:?}"
        );
        let second = entries.next(&token).await;
        anyhow::ensure!(second == Ok(None), "expected end, got {second:?}");
        Ok(())
    }

    #[rudzio::test]
    async fn kv_entries_yield_cancelled(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        token.cancel();
        let mut storage = Storage;
        let mut entries = storage.entries();
        let first = entries.next(&token).await;
        anyhow::ensure!(
            first == Err(NextEntryError::Cancelled),
            "expected Cancelled, got {first:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn end_of_input_is_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let mut storage = Storage;
        let notified = storage.notify().await;
        anyhow::ensure!(
            notified == Err(EndOfInputError::Unimplemented),
            "expected Unimplemented, got {notified:?}"
        );
        let waited = storage.wait().await;
        anyhow::ensure!(
            waited == Err(EndOfInputError::Unimplemented),
            "expected Unimplemented, got {waited:?}"
        );
        Ok(())
    }
}
