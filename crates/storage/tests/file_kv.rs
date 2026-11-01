//! File-backed (memory-mapped) key-value tests against the real backing.

use std::path::PathBuf;

use rudzio::common::context::Suite;
use rudzio::runtime::compio::Runtime;

/// Byte length of the fixed reservation each test maps.
const CAPACITY: usize = 1 << 20;

/// Path of a per-test temp backing file, unique per process and test.
fn path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "record-processor-storage-{name}-{}.kv",
        std::process::id()
    ))
}

#[rudzio::suite([(runtime = Runtime::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use allocator_api2::alloc::Global;
    use std::hash::RandomState;
    use std::path::PathBuf;
    use storage::file_kv::{FetchError, Parts, WriteError};
    use storage::{FileKv, Kv as _};
    use tokio_util::sync::CancellationToken;

    use super::{CAPACITY, path};

    /// Open and size the backing file, then map it into a store of `u16` keys
    /// and `u32` values.
    async fn kv(name: &str) -> anyhow::Result<(FileKv<u16, u32, RandomState, Global>, PathBuf)> {
        let backing = path(name);
        let file = compio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&backing)
            .await?;
        file.set_len(u64::try_from(CAPACITY)?).await?;
        let store = FileKv::try_from(Parts {
            alloc: Global,
            hasher: RandomState::new(),
            file,
            capacity: CAPACITY,
        })?;
        Ok((store, backing))
    }

    #[rudzio::test]
    async fn load_missing_is_none(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let (kv, backing) = kv("load_missing").await?;
        let cancel = CancellationToken::new();
        let loaded = kv.load(1, &cancel).await;
        anyhow::ensure!(loaded == Ok(None), "expected Ok(None), got {loaded:?}");
        compio::fs::remove_file(&backing).await?;
        Ok(())
    }

    #[rudzio::test]
    async fn load_returns_stored_value(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let (kv, backing) = kv("load_present").await?;
        let cancel = CancellationToken::new();
        kv.store(1, 10, &cancel).await?;
        kv.store(2, 20, &cancel).await?;
        let first = kv.load(1, &cancel).await;
        let second = kv.load(2, &cancel).await;
        anyhow::ensure!(
            (first, second) == (Ok(Some(10)), Ok(Some(20))),
            "unexpected loads {first:?} {second:?}"
        );
        compio::fs::remove_file(&backing).await?;
        Ok(())
    }

    #[rudzio::test]
    async fn store_overwrites_value(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let (kv, backing) = kv("overwrite").await?;
        let cancel = CancellationToken::new();
        kv.store(1, 10, &cancel).await?;
        kv.store(1, 11, &cancel).await?;
        let loaded = kv.load(1, &cancel).await;
        anyhow::ensure!(
            loaded == Ok(Some(11)),
            "expected Ok(Some(11)), got {loaded:?}"
        );
        compio::fs::remove_file(&backing).await?;
        Ok(())
    }

    #[rudzio::test]
    async fn next_key_is_ascending(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let (kv, backing) = kv("next_key").await?;
        let cancel = CancellationToken::new();
        kv.store(3, 30, &cancel).await?;
        kv.store(1, 10, &cancel).await?;
        kv.store(2, 20, &cancel).await?;
        let first = kv.next_key(None, &cancel).await;
        let second = kv.next_key(Some(1), &cancel).await;
        let third = kv.next_key(Some(2), &cancel).await;
        let last = kv.next_key(Some(3), &cancel).await;
        anyhow::ensure!(
            (first, second, third, last) == (Ok(Some(1)), Ok(Some(2)), Ok(Some(3)), Ok(None)),
            "unexpected enumeration {first:?} {second:?} {third:?} {last:?}"
        );
        compio::fs::remove_file(&backing).await?;
        Ok(())
    }

    #[rudzio::test]
    async fn cancelled_operations_are_rejected(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let (kv, backing) = kv("cancelled").await?;
        let cancel = CancellationToken::new();
        cancel.cancel();
        let stored = kv.store(1, 10, &cancel).await;
        let loaded = kv.load(1, &cancel).await;
        anyhow::ensure!(
            (&stored, &loaded) == (&Err(WriteError::Cancelled), &Err(FetchError::Cancelled)),
            "unexpected cancellation results {stored:?} {loaded:?}"
        );
        compio::fs::remove_file(&backing).await?;
        Ok(())
    }
}
