//! In-memory key-value tests against the real backing.

use rudzio::common::context::Suite;
use rudzio::runtime::compio::Runtime;

#[rudzio::suite([(runtime = Runtime::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use std::hash::RandomState;

    use allocator_api2::alloc::Global;
    use storage::memory_kv::{LoadError, Parts, StoreError};
    use storage::{Kv as _, MemoryKv};
    use tokio_util::sync::CancellationToken;

    /// Build an empty store of `u16` keys and `u32` values.
    fn kv() -> MemoryKv<u16, u32, RandomState, Global> {
        MemoryKv::from(Parts {
            alloc: Global,
            hasher: RandomState::new(),
        })
    }

    #[rudzio::test]
    async fn load_missing_is_none(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let kv = kv();
        let cancel = CancellationToken::new();
        let loaded = kv.load(1, &cancel).await;
        anyhow::ensure!(loaded == Ok(None), "expected Ok(None), got {loaded:?}");
        Ok(())
    }

    #[rudzio::test]
    async fn load_returns_stored_value(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let kv = kv();
        let cancel = CancellationToken::new();
        kv.store(1, 10, &cancel).await?;
        kv.store(2, 20, &cancel).await?;
        let first = kv.load(1, &cancel).await;
        let second = kv.load(2, &cancel).await;
        anyhow::ensure!(
            (first, second) == (Ok(Some(10)), Ok(Some(20))),
            "unexpected loads {first:?} {second:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn store_overwrites_value(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let kv = kv();
        let cancel = CancellationToken::new();
        kv.store(1, 10, &cancel).await?;
        kv.store(1, 11, &cancel).await?;
        let loaded = kv.load(1, &cancel).await;
        anyhow::ensure!(
            loaded == Ok(Some(11)),
            "expected Ok(Some(11)), got {loaded:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn cancelled_operations_are_rejected(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let kv = kv();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let stored = kv.store(1, 10, &cancel).await;
        let loaded = kv.load(1, &cancel).await;
        anyhow::ensure!(
            (&stored, &loaded) == (&Err(StoreError::Cancelled), &Err(LoadError::Cancelled)),
            "unexpected cancellation results {stored:?} {loaded:?}"
        );
        Ok(())
    }
}
