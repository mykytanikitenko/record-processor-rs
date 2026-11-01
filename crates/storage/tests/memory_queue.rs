//! In-memory queue tests against the real backing.

use rudzio::common::context::Suite;
use rudzio::runtime::compio::Runtime;

#[rudzio::suite([(runtime = Runtime::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use allocator_api2::alloc::Global;
    use futures_util::future::join;
    use storage::memory_queue::{MemoryAppendError, MemoryNextBatchError, Parts};
    use storage::{MemoryQueue, Offset, Queue as _};
    use tokio_util::sync::CancellationToken;

    /// Build an empty queue of `u32` records in the global allocator.
    fn queue() -> MemoryQueue<u32, Global> {
        MemoryQueue::from(Parts { alloc: Global })
    }

    #[rudzio::test]
    async fn append_returns_offset_of_first_record(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = queue();
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
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = queue();
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
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = queue();
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
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = queue();
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
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = queue();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let appended = queue.append(&[1], &cancel).await;
        anyhow::ensure!(
            appended == Err(MemoryAppendError::Cancelled),
            "expected Cancelled, got {appended:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn cancellation_ends_waiting_next_batch(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let queue = queue();
        let cancel = CancellationToken::new();
        let (batch, ()) = join(queue.next_batch(Offset::from(0), &cancel), async {
            cancel.cancel();
        })
        .await;
        anyhow::ensure!(
            batch == Err(MemoryNextBatchError::Cancelled),
            "expected Cancelled, got {batch:?}"
        );
        Ok(())
    }
}
