//! The in-memory queue backing.
//!
//! Records live in segments, one per appended batch. A segment is allocated
//! once with the exact batch length and never grows, and the segment directory
//! is a fixed array of levels whose slot vectors are allocated once at their
//! final length, so a published record never moves. That stability is what
//! lets [`Queue::next_batch`] hand out slices borrowing the queue itself.

use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use allocator_api2::alloc::Allocator;
use allocator_api2::collections::TryReserveError;
use allocator_api2::vec::Vec;
use event_listener::Event;
use tokio_util::sync::CancellationToken;

use crate::offset::Offset;
use crate::queue::Queue;

/// Number of directory levels; level `n` holds `2^n` segments, so the
/// directory addresses `2^LEVELS - 1` segments in total.
const LEVELS: usize = 32;

/// One appended batch and the offset of its first record.
#[derive(Debug)]
struct Segment<T, A>
where
    A: Allocator,
{
    /// The batch records, allocated at their exact length.
    records: Vec<T, A>,
    /// The offset of the first record.
    start: Offset,
}

/// One directory level: a slot vector allocated once at its final length.
type Level<T, A> = Vec<OnceLock<Segment<T, A>>, A>;

/// The position of a segment inside the directory.
#[derive(Clone, Copy, Debug)]
struct Slot {
    /// The directory level.
    level: usize,
    /// The index inside the level.
    index: usize,
}

/// Failure to locate a segment slot in the directory.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum SlotError {
    /// The segment number is beyond the directory capacity.
    #[error("segment {segment} exceeds the queue directory capacity")]
    Capacity {
        /// The segment number that does not fit.
        segment: u64,
    },
}

impl TryFrom<u64> for Slot {
    type Error = SlotError;

    #[inline]
    fn try_from(segment: u64) -> Result<Self, Self::Error> {
        let capacity = SlotError::Capacity { segment };
        let position = segment.checked_add(1).ok_or(capacity)?;
        let level = position.ilog2();
        let first = 1_u64.checked_shl(level).ok_or(capacity)?;
        let index = position.checked_sub(first).ok_or(capacity)?;
        let level = usize::try_from(level).map_err(|_overflow| capacity)?;
        let index = usize::try_from(index).map_err(|_overflow| capacity)?;
        if level >= LEVELS {
            return Err(capacity);
        }
        Ok(Self { level, index })
    }
}

/// Construction input for [`MemoryQueue`].
#[derive(Clone, Copy, Debug)]
pub struct Parts<A> {
    /// The allocator every segment and directory level is allocated in.
    pub alloc: A,
}

/// Bookkeeping serialised across appends.
#[derive(Debug)]
struct Tail {
    /// The number of published segments.
    segments: u64,
    /// The offset one past the last appended record.
    next: u64,
}

/// The in-memory queue backing, allocating in the caller's `A` allocator.
#[derive(Debug)]
pub struct MemoryQueue<T, A>
where
    A: Allocator,
{
    /// The allocator new segments and levels are allocated in.
    alloc: A,
    /// Signalled after every publication so waiting readers re-check.
    appended: Event,
    /// The directory of segment levels.
    levels: [OnceLock<Level<T, A>>; LEVELS],
    /// The number of published segments, read without the append lock.
    published: AtomicU64,
    /// The append bookkeeping, held across a whole append.
    tail: async_lock::Mutex<Tail>,
}

impl<T, A> From<Parts<A>> for MemoryQueue<T, A>
where
    A: Allocator,
{
    #[inline]
    fn from(parts: Parts<A>) -> Self {
        Self {
            alloc: parts.alloc,
            appended: Event::new(),
            levels: [const { OnceLock::new() }; LEVELS],
            published: AtomicU64::new(0),
            tail: async_lock::Mutex::new(Tail {
                segments: 0,
                next: 0,
            }),
        }
    }
}

/// Failure to look up a published segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum SegmentError {
    /// The segment number does not map to a directory slot.
    #[error(transparent)]
    Slot(#[from] SlotError),
    /// The segment is counted as published but its slot is empty.
    #[error("segment {segment} is counted as published but is missing")]
    Missing {
        /// The missing segment number.
        segment: u64,
    },
}

impl<T, A> MemoryQueue<T, A>
where
    A: Allocator,
{
    /// Return the published segment numbered `segment`.
    fn segment(&self, segment: u64) -> Result<&Segment<T, A>, SegmentError> {
        let missing = SegmentError::Missing { segment };
        let slot = Slot::try_from(segment)?;
        let level = self.levels.get(slot.level).ok_or(missing)?;
        level
            .get()
            .and_then(|slots| slots.get(slot.index))
            .and_then(OnceLock::get)
            .ok_or(missing)
    }
}

/// Failure to find the segment holding an offset.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum FindError {
    /// A published segment could not be read.
    #[error(transparent)]
    Segment(#[from] SegmentError),
    /// The search bounds overflowed.
    #[error("segment search overflowed")]
    Overflow,
}

impl<T, A> MemoryQueue<T, A>
where
    A: Allocator,
{
    /// Return the records from `since` to the end of its segment, or `None`
    /// when `since` is at or past the published tail.
    fn find(&self, since: Offset, published: u64) -> Result<Option<&[T]>, FindError> {
        let mut low = 0_u64;
        let mut high = published;
        while low < high {
            let span = high.checked_sub(low).ok_or(FindError::Overflow)?;
            let middle = low.checked_add(span / 2).ok_or(FindError::Overflow)?;
            let segment = self.segment(middle)?;
            if segment.start <= since {
                low = middle.checked_add(1).ok_or(FindError::Overflow)?;
            } else {
                high = middle;
            }
        }
        let Some(last) = low.checked_sub(1) else {
            return Ok(None);
        };
        let segment = self.segment(last)?;
        let skip = since.checked_sub(*segment.start).ok_or(FindError::Overflow)?;
        let skip = usize::try_from(skip).map_err(|_overflow| FindError::Overflow)?;
        Ok(segment.records.get(skip..).filter(|records| !records.is_empty()))
    }
}

/// Failure to append a batch to the in-memory queue.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum AppendError {
    /// The append was cancelled before it published the batch.
    #[error("append cancelled")]
    Cancelled,
    /// The segment or directory level could not be allocated.
    #[error("queue allocation failed: {0}")]
    Alloc(#[from] TryReserveError),
    /// The record or segment count overflowed.
    #[error("queue offset overflowed")]
    Overflow,
    /// The next segment does not map to a directory slot.
    #[error(transparent)]
    Slot(#[from] SlotError),
    /// The next segment slot was already filled.
    #[error("segment {segment} was already published")]
    Occupied {
        /// The occupied segment number.
        segment: u64,
    },
}

/// Failure to read the next batch from the in-memory queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum NextBatchError {
    /// The wait for records was cancelled.
    #[error("next batch cancelled")]
    Cancelled,
    /// The published segments could not be searched.
    #[error(transparent)]
    Find(#[from] FindError),
}

impl<T, A> MemoryQueue<T, A>
where
    T: Copy,
    A: Allocator + Clone,
{
    /// Publish `batch` as the next segment under the append lock.
    async fn publish(&self, batch: &[T]) -> Result<Offset, AppendError> {
        let mut tail = self.tail.lock().await;
        let start = tail.next;
        if batch.is_empty() {
            return Ok(Offset::from(start));
        }
        let length = u64::try_from(batch.len()).map_err(|_overflow| AppendError::Overflow)?;
        let next = start.checked_add(length).ok_or(AppendError::Overflow)?;
        let number = tail.segments;
        let segments = number.checked_add(1).ok_or(AppendError::Overflow)?;
        let slot = Slot::try_from(number)?;
        let occupied = AppendError::Occupied { segment: number };
        let level = self.levels.get(slot.level).ok_or(occupied.clone())?;
        if level.get().is_none() {
            let width = 1_usize
                .checked_shl(u32::try_from(slot.level).map_err(|_overflow| AppendError::Overflow)?)
                .ok_or(AppendError::Overflow)?;
            let mut slots = Vec::new_in(self.alloc.clone());
            slots.try_reserve_exact(width)?;
            slots.resize_with(width, OnceLock::new);
            level.set(slots).map_err(|_level| occupied.clone())?;
        }
        let mut records = Vec::new_in(self.alloc.clone());
        records.try_reserve_exact(batch.len())?;
        records.extend_from_slice(batch);
        let segment = Segment {
            records,
            start: Offset::from(start),
        };
        level
            .get()
            .and_then(|slots| slots.get(slot.index))
            .ok_or(occupied.clone())?
            .set(segment)
            .map_err(|_segment| occupied)?;
        tail.segments = segments;
        tail.next = next;
        self.published.store(segments, Ordering::Release);
        let _woken: usize = self.appended.notify(usize::MAX);
        Ok(Offset::from(start))
    }

    /// Wait until the record at `since` is published and return its slice.
    async fn wait(&self, since: Offset) -> Result<&[T], NextBatchError> {
        loop {
            let listener = self.appended.listen();
            let published = self.published.load(Ordering::Acquire);
            if let Some(records) = self.find(since, published)? {
                return Ok(records);
            }
            listener.await;
        }
    }
}

impl<T, A> Queue<T> for MemoryQueue<T, A>
where
    T: Copy,
    A: Allocator + Clone,
{
    type AppendError = AppendError;
    type NextBatchError = NextBatchError;

    #[inline]
    async fn append(
        &self,
        batch: &[T],
        cancel: &CancellationToken,
    ) -> Result<Offset, Self::AppendError> {
        cancel
            .run_until_cancelled(self.publish(batch))
            .await
            .ok_or(AppendError::Cancelled)?
    }

    #[inline]
    async fn next_batch<'queue>(
        &'queue self,
        since: Offset,
        cancel: &CancellationToken,
    ) -> Result<&'queue [T], Self::NextBatchError>
    where
        T: 'queue,
    {
        cancel
            .run_until_cancelled(self.wait(since))
            .await
            .ok_or(NextBatchError::Cancelled)?
    }
}
