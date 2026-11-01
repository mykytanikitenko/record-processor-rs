//! The file-backed queue, memory-mapped for zero-copy reads.
//!
//! Records live in a single memory-mapped region that is reserved once and
//! never remapped, so a published record never moves and [`Queue::next_batch`]
//! can hand out slices that borrow the queue itself. Appends copy records into
//! the tail of the mapping; the backing file is opened, sized to the fixed
//! reservation, and kept alive by the queue for the mapping's lifetime.
//!
//! The only `unsafe` here is the raw memory mapping itself and the typed
//! reinterpretation of its bytes as `[T]`; both are confined to this module
//! and documented with `SAFETY` notes.

// The raw memory-mapped bytes are reinterpreted as `[T]` via raw pointers; this
// module is the single, well-documented unsafe boundary of the storage crate.
// The memory mapping cannot be established without `unsafe`, permitted by the
// root `[workspace.lints.rust]` table.

use core::cell::RefCell;
use core::marker::PhantomData;
use std::path::PathBuf;

use allocator_api2::alloc::Allocator;
use allocator_api2::vec::Vec;
use compio::buf::memmap2::{MmapMut, MmapOptions};
use compio::driver::AsRawFd as _;
use compio::fs::File;
use event_listener::Event;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::offset::Offset;
use crate::queue::Queue;

/// The smallest page size on any supported target, in bytes.
///
/// `mmap` returns page-aligned addresses, so a `T` whose alignment is at most
/// this value is always correctly aligned inside the mapping.
const MIN_PAGE_ALIGN: usize = 4096;

/// Failure to open the file-backed queue.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OpenError {
    /// The backing file could not be mapped.
    #[error("mapping the backing file failed: {0}")]
    Mmap(#[from] std::io::Error),
    /// The record type requires more alignment than the mapping guarantees.
    #[error(
        "record type requires alignment {align}, exceeding the mapping's {MIN_PAGE_ALIGN}-byte alignment"
    )]
    Unaligned {
        /// The record type's required alignment, in bytes.
        align: usize,
    },
}

/// Failure to append a batch to the file-backed queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum FileAppendError {
    /// The append was cancelled before it published the batch.
    #[error("append cancelled")]
    Cancelled,
    /// The append would exceed the fixed reservation.
    #[error("queue capacity exceeded")]
    Capacity,
    /// The record or byte count overflowed.
    #[error("queue offset overflowed")]
    Overflow,
}

/// Failure to read the next batch from the file-backed queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum FileNextBatchError {
    /// The wait for records was cancelled.
    #[error("next batch cancelled")]
    Cancelled,
    /// The published records could not be located.
    #[error(transparent)]
    Find(#[from] FileFindError),
}

/// Failure to locate the segment holding an offset.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum FileFindError {
    /// The search bounds overflowed.
    #[error("segment search overflowed")]
    Overflow,
}

/// The file-backed queue, memory-mapping a fixed reservation.
#[derive(Debug)]
pub struct FileQueue<T, A>
where
    A: Allocator,
{
    /// The backing file, kept alive because the mapping borrows its descriptor.
    _file: File,
    /// The single memory-mapped region holding every published record.
    _map: MmapMut,
    /// Signalled after every publication so waiting readers re-check.
    appended: Event,
    /// The cached base address of the mapping; never remapped, so stable.
    base: *mut u8,
    /// The byte length of the mapping (the fixed reservation).
    capacity: usize,
    /// Marks the record type the byte mapping is interpreted as.
    marker: PhantomData<T>,
    /// Path of the backing file, removed by the drop-scheduled cleanup; taken
    /// by [`Drop`] to schedule the removal, leaving `None` behind.
    path: Option<PathBuf>,
    /// The start offset of every published segment, in records.
    starts: RefCell<Vec<u64, A>>,
    /// The number of published records.
    tail: RefCell<u64>,
    /// Shared tracker the drop-scheduled removal registers with.
    tracker: TaskTracker,
}

impl<T, A> Drop for FileQueue<T, A>
where
    A: Allocator,
{
    #[inline]
    fn drop(&mut self) {
        // `Drop` is synchronous and cannot await, so the backing file's
        // removal is detached onto the current compio runtime and tracked by
        // the shared tracker. The tracker owner (the CLI, or the test suite)
        // closes and waits on it before exiting, guaranteeing the file is
        // removed on every path — success, error, and cancellation alike.
        if let Some(path) = self.path.take() {
            compio::runtime::spawn(self.tracker.track_future(async move {
                drop(compio::fs::remove_file(path).await);
            }))
            .detach();
        }
    }
}

impl<T, A> FileQueue<T, A>
where
    T: Copy,
    A: Allocator,
{
    /// Return the records from `since` to the end of its segment, or `None`
    /// when `since` is at or past the published tail.
    fn find(&self, since: Offset, tail: u64) -> Result<Option<&[T]>, FileFindError> {
        let start = *since;
        if start >= tail {
            return Ok(None);
        }
        let starts = self.starts.borrow();
        let segment = starts.partition_point(|&segment_start| segment_start <= start);
        // `segment >= 1` because the first published segment starts at zero.
        let end = if let Some(&next) = starts.get(segment) {
            next
        } else {
            tail
        };
        let len_u64 = end.checked_sub(start).ok_or(FileFindError::Overflow)?;
        let len = usize::try_from(len_u64).map_err(|_overflow| FileFindError::Overflow)?;
        let start_bytes = usize::try_from(start)
            .map_err(|_overflow| FileFindError::Overflow)?
            .checked_mul(size_of::<T>())
            .ok_or(FileFindError::Overflow)?;
        // SAFETY: `start..end` is a published, file-backed range inside the
        // mapping, so its bytes are initialized and valid for `len` `T`s. The
        // base pointer is stable (the mapping is never remapped) and is aligned
        // for `T` (checked at construction). Appends only ever write the
        // unpublished tail, never this published range, so no `&mut` aliases it.
        // SAFETY: `base` is a stable, aligned pointer into the file-backed
        // mapping, and `start_bytes` is within the published range.
        let ptr = unsafe { self.base.add(start_bytes) }.cast::<T>();
        // SAFETY: the `len` bytes from `ptr` are published and file-backed, so
        // they hold initialized, correctly-aligned `T`s.
        let records = unsafe { core::slice::from_raw_parts(ptr, len) };
        Ok(Some(records))
    }

    /// Publish `batch` at the tail of the mapping.
    /// Publish `batch` at the tail of the mapping.
    async fn publish(&self, batch: &[T]) -> Result<Offset, FileAppendError> {
        let mut tail = self.tail.borrow_mut();
        let start = *tail;
        if batch.is_empty() {
            return Ok(Offset::from(start));
        }
        let count = u64::try_from(batch.len()).map_err(|_overflow| FileAppendError::Overflow)?;
        let end = start.checked_add(count).ok_or(FileAppendError::Overflow)?;
        let byte_count = batch
            .len()
            .checked_mul(size_of::<T>())
            .ok_or(FileAppendError::Overflow)?;
        let start_bytes = usize::try_from(start)
            .map_err(|_overflow| FileAppendError::Overflow)?
            .checked_mul(size_of::<T>())
            .ok_or(FileAppendError::Overflow)?;
        let end_bytes = start_bytes
            .checked_add(byte_count)
            .ok_or(FileAppendError::Overflow)?;
        if end_bytes > self.capacity {
            return Err(FileAppendError::Capacity);
        }
        // SAFETY: the destination range `[start_bytes, end_bytes)` lies within
        // the mapping (checked above) and is aligned for `T`; the source is the
        // caller's `batch`. The ranges are disjoint from every published record
        // (the destination is the unpublished tail), so no aliasing occurs.
        // SAFETY: `base` is a stable, aligned pointer into the file-backed
        // mapping, and `start_bytes` is the unpublished tail offset.
        let dst = unsafe { self.base.add(start_bytes) };
        let src = batch.as_ptr().cast::<u8>();
        // SAFETY: `src` is a valid pointer to `byte_count` bytes (`batch`), and
        // `dst` points to the unpublished tail of the mapping; the regions are
        // disjoint from every live `&[T]`.
        unsafe {
            core::ptr::copy_nonoverlapping(src, dst, byte_count);
        }
        self.starts.borrow_mut().push(start);
        *tail = end;
        drop(tail);
        let _woken: usize = self.appended.notify(usize::MAX);
        Ok(Offset::from(start))
    }

    /// Wait until the record at `since` is published and return its slice.
    /// Wait until the record at `since` is published and return its slice.
    async fn wait(&self, since: Offset) -> Result<&[T], FileNextBatchError> {
        loop {
            let listener = self.appended.listen();
            let tail = *self.tail.borrow();
            if let Some(records) = self.find(since, tail)? {
                return Ok(records);
            }
            listener.await;
        }
    }
}

impl<T, A> TryFrom<Parts<A>> for FileQueue<T, A>
where
    T: Copy,
    A: Allocator + Copy,
{
    type Error = OpenError;

    #[inline]
    fn try_from(parts: Parts<A>) -> Result<Self, Self::Error> {
        let Parts {
            alloc,
            file,
            capacity,
            tracker,
            path,
        } = parts;
        let align = align_of::<T>();
        if align > MIN_PAGE_ALIGN {
            return Err(OpenError::Unaligned { align });
        }
        // SAFETY: `file.as_raw_fd()` names an open file that the caller has
        // sized to exactly `capacity` bytes and will not resize while the queue
        // is live; the queue keeps `file` alive for the mapping's lifetime, so
        // the descriptor stays valid and the mapped pages stay file-backed.
        let mut map = unsafe { MmapOptions::new().len(capacity).map_mut(file.as_raw_fd())? };
        let base = map.as_mut_ptr();
        Ok(Self {
            _file: file,
            _map: map,
            base,
            capacity,
            appended: Event::new(),
            starts: RefCell::new(Vec::new_in(alloc)),
            tail: RefCell::new(0),
            marker: PhantomData,
            path: Some(path),
            tracker,
        })
    }
}

impl<T, A> Queue<T> for FileQueue<T, A>
where
    T: Copy,
    A: Allocator + Copy,
{
    type AppendError = FileAppendError;
    type NextBatchError = FileNextBatchError;

    #[inline]
    async fn append(
        &self,
        batch: &[T],
        cancel: &CancellationToken,
    ) -> Result<Offset, Self::AppendError> {
        cancel
            .run_until_cancelled(self.publish(batch))
            .await
            .ok_or(FileAppendError::Cancelled)?
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
            .ok_or(FileNextBatchError::Cancelled)?
    }
}

/// Construction input for [`FileQueue`].
#[derive(Debug)]
pub struct Parts<A> {
    /// The allocator the segment index is allocated in.
    pub alloc: A,
    /// The byte length of the reservation, equal to the file's length.
    pub capacity: usize,
    /// The already-opened backing file.
    ///
    /// The caller must have opened it read-write and extended it to exactly
    /// `capacity` bytes with [`File::set_len`] before construction, and must
    /// not resize it while the queue is live.
    pub file: File,
    /// Path of the backing file, removed by the drop-scheduled cleanup.
    pub path: PathBuf,
    /// Shared tracker the drop-scheduled removal registers with.
    pub tracker: TaskTracker,
}
