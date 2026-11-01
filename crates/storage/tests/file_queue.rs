//! File-backed (memory-mapped) queue tests against the real backing.

use std::path::PathBuf;

use rudzio::common::context::Suite;
use rudzio::runtime::compio::Runtime;

/// Byte length of the fixed reservation each test maps.
const CAPACITY: usize = 1 << 20;

/// Path of a per-test temp backing file, unique per process and test.
fn path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "record-processor-storage-{name}-{}.mmap",
        std::process::id()
    ))
}

#[rudzio::suite([(runtime = Runtime::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use allocator_api2::alloc::Global;
    use futures_util::future::join;
    use rudzio::context::Test as _;
    use storage::file_queue::{FileAppendError, FileNextBatchError, Parts};
    use storage::{FileQueue, Offset, Queue as _};
    use tokio_util::sync::CancellationToken;
    use tokio_util::task::TaskTracker;

    use super::{CAPACITY, path};

    /// Open and size the backing file, then map it into a queue of `u32`s.
    async fn queue(name: &str, tracker: &TaskTracker) -> anyhow::Result<FileQueue<u32, Global>> {
        let backing = path(name);
        let file = compio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&backing)
            .await?;
        file.set_len(u64::try_from(CAPACITY)?).await?;
        let queue = FileQueue::try_from(Parts {
            alloc: Global,
            file,
            path: backing,
            tracker: tracker.clone(),
            capacity: CAPACITY,
        })?;
        Ok(queue)
    }

    #[rudzio::test]
    async fn append_returns_offset_of_first_record(
        ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = queue("append_returns_offset", ctx.tracker()).await?;
        let cancel = CancellationToken::new();
        let first = queue.append(&[10, 11, 12], &cancel).await?;
        let second = queue.append(&[20, 21], &cancel).await?;
        let empty = queue.append(&[], &cancel).await?;
        let third = queue.append(&[30], &cancel).await?;
        anyhow::ensure!(
            (first, second, empty, third)
                == (
                    Offset::from(0),
                    Offset::from(3),
                    Offset::from(5),
                    Offset::from(5)
                ),
            "unexpected offsets {first:?} {second:?} {empty:?} {third:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn next_batch_from_zero_and_mid_segment(
        ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = queue("next_batch_from_zero", ctx.tracker()).await?;
        let cancel = CancellationToken::new();
        let _first: Offset = queue.append(&[10, 11, 12], &cancel).await?;
        let _second: Offset = queue.append(&[20, 21], &cancel).await?;
        let from_zero = queue.next_batch(Offset::from(0), &cancel).await?;
        let mid_first = queue.next_batch(Offset::from(1), &cancel).await?;
        let second_start = queue.next_batch(Offset::from(3), &cancel).await?;
        let second_mid = queue.next_batch(Offset::from(4), &cancel).await?;
        anyhow::ensure!(
            (from_zero, mid_first, second_start, second_mid)
                == (&[10, 11, 12][..], &[11, 12][..], &[20, 21][..], &[21][..]),
            "unexpected batches {from_zero:?} {mid_first:?} {second_start:?} {second_mid:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn next_batch_wakes_after_append(
        ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = queue("next_batch_wakes", ctx.tracker()).await?;
        let cancel = CancellationToken::new();
        let (batch_result, appended_result) = join(
            queue.next_batch(Offset::from(0), &cancel),
            queue.append(&[7, 8], &cancel),
        )
        .await;
        let batch = batch_result?;
        let appended = appended_result?;
        anyhow::ensure!(
            (batch, appended) == (&[7, 8][..], Offset::from(0)),
            "unexpected wake result {batch:?} {appended:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn slices_stay_valid_across_many_segments(
        ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = queue("slices_stay_valid", ctx.tracker()).await?;
        let cancel = CancellationToken::new();
        let _head: Offset = queue.append(&[1, 2, 3], &cancel).await?;
        let early = queue.next_batch(Offset::from(1), &cancel).await?;
        for _segment in 0_u32..300 {
            let _offset: Offset = queue.append(&[7, 8], &cancel).await?;
        }
        let tail = queue.next_batch(Offset::from(601), &cancel).await?;
        anyhow::ensure!(
            (early, tail) == (&[2, 3][..], &[7, 8][..]),
            "unexpected slices {early:?} {tail:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn cancelled_append_is_rejected(
        ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = queue("cancelled_append", ctx.tracker()).await?;
        let cancel = CancellationToken::new();
        cancel.cancel();
        let appended = queue.append(&[1], &cancel).await;
        anyhow::ensure!(
            appended == Err(FileAppendError::Cancelled),
            "expected Cancelled, got {appended:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn cancellation_ends_waiting_next_batch(
        ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = queue("cancellation_ends_wait", ctx.tracker()).await?;
        let cancel = CancellationToken::new();
        let (batch, ()) = join(queue.next_batch(Offset::from(0), &cancel), async {
            cancel.cancel();
        })
        .await;
        anyhow::ensure!(
            batch == Err(FileNextBatchError::Cancelled),
            "expected Cancelled, got {batch:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn drop_schedules_backing_file_removal(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        // A dedicated tracker lets this test close-and-wait to assert removal
        // deterministically; the suite-shared tracker the other tests reuse
        // only drains at suite teardown.
        let tracker = TaskTracker::new();
        let backing = path("drop_schedules_removal");
        let queue = queue("drop_schedules_removal", &tracker).await?;
        anyhow::ensure!(backing.exists(), "backing file should exist before drop");
        drop(queue);
        let _closed: bool = tracker.close();
        tracker.wait().await;
        anyhow::ensure!(
            !backing.exists(),
            "backing file should be removed after drop"
        );
        Ok(())
    }
}
