//! Storage integration tests against the production backend's trait API.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use record_processor_domain::client_id::ClientId;
    use record_processor_domain::input::{Transaction, TxId};
    use record_processor_domain::output::{AccountBalance, Balance, Parts};
    use storage::{
        DedupIndex as _, EndOfInput as _, EventQueue as _, IndexInsertError, KvState as _,
        NextEntryError, QueueAppendError, QueueReadError, SignalNotifyError, SignalWaitError,
        StateReadError, StateWriteError, Storage,
    };
    use tokio_util::sync::CancellationToken;

    #[rudzio::test]
    async fn event_queue_append_batch_is_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut storage = Storage;
        let appended = storage.append_batch(&[Transaction::Eof], &token).await;
        anyhow::ensure!(
            appended == Err(QueueAppendError::Unimplemented),
            "expected Unimplemented, got {appended:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn event_queue_read_batch_is_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut storage = Storage;
        let mut events = [Transaction::Eof; 4];
        let read = storage.read_batch(0, &mut events, &token).await;
        anyhow::ensure!(
            read == Err(QueueReadError::Unimplemented),
            "expected Unimplemented, got {read:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn dedup_insert_is_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut storage = Storage;
        let inserted = storage.insert(TxId::from(1), &token).await;
        anyhow::ensure!(
            inserted == Err(IndexInsertError::Unimplemented),
            "expected Unimplemented, got {inserted:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn end_of_input_is_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut storage = Storage;
        let notified = storage.notify(0, &token).await;
        anyhow::ensure!(
            notified == Err(SignalNotifyError::Unimplemented),
            "expected Unimplemented, got {notified:?}"
        );
        let waited = storage.wait(&token).await;
        anyhow::ensure!(
            waited == Err(SignalWaitError::Unimplemented),
            "expected Unimplemented, got {waited:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn kv_state_is_unimplemented(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut storage = Storage;
        let balance = AccountBalance::try_from(Parts {
            client: ClientId::from(1),
            available: Balance::from(rust_decimal::Decimal::ONE),
            held: Balance::from(rust_decimal::Decimal::ZERO),
            locked: false,
        })?;
        let written = storage.write(ClientId::from(1), &balance, &token).await;
        anyhow::ensure!(
            written == Err(StateWriteError::Unimplemented),
            "expected Unimplemented, got {written:?}"
        );
        let read = storage.read(ClientId::from(1), &token).await;
        anyhow::ensure!(
            read == Err(StateReadError::Unimplemented),
            "expected Unimplemented, got {read:?}"
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
}
