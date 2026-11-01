//! The file-backed key-value store, memory-mapped for zero-copy reads.
//!
//! Records are `(key, value)` tuples appended to a single memory-mapped region
//! that is reserved once and never remapped, so a stored value never moves. A
//! side index maps each key to the byte offset of its latest record; [`Kv::store`]
//! appends a fresh record and repoints the key at it, so an overwrite leaves the
//! superseded bytes in place and only the index changes. [`Kv::load`] reads the
//! indexed record back out of the mapping.
//!
//! The only `unsafe` here is the raw memory mapping itself and the typed
//! reinterpretation of its bytes as `(K, V)`; both are confined to this module
//! and documented with `SAFETY` notes.

// The mapping's bytes are reinterpreted as `(K, V)` via raw pointers; this is
// the single, well-documented unsafe boundary of the storage crate. The memory
// mapping cannot be established without `unsafe`, permitted by the root
// `[workspace.lints.rust]` table.

use core::cell::RefCell;
use core::hash::{BuildHasher, Hash};
use core::marker::PhantomData;

use allocator_api2::alloc::Allocator;
use compio::buf::memmap2::{MmapMut, MmapOptions};
use compio::driver::AsRawFd as _;
use compio::fs::File;
use hashbrown::HashMap;
use tokio_util::sync::CancellationToken;

use crate::kv::Kv;

/// The smallest page size on any supported target, in bytes.
///
/// `mmap` returns page-aligned addresses, so a record whose alignment is at
/// most this value is always correctly aligned inside the mapping.
const MIN_PAGE_ALIGN: usize = 4096;

/// Failure to enumerate the keys of the file-backed key-value store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum EnumerateError {
    /// The key enumeration was cancelled before the index was read.
    #[error("key enumeration cancelled")]
    Cancelled,
}

/// Failure to load a value from the file-backed key-value store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum FetchError {
    /// The load was cancelled before the index was read.
    #[error("load cancelled")]
    Cancelled,
}

/// Failure to open the file-backed key-value store.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MapError {
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

/// Failure to store a value in the file-backed key-value store.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum WriteError {
    /// The index could not grow to hold the new key.
    #[error("key-value index allocation failed: {0}")]
    Alloc(#[from] hashbrown::TryReserveError),
    /// The store was cancelled before the record was appended.
    #[error("store cancelled")]
    Cancelled,
    /// The append would exceed the fixed reservation.
    #[error("key-value store capacity exceeded")]
    Capacity,
    /// The byte cursor overflowed.
    #[error("key-value store offset overflowed")]
    Overflow,
}

/// The file-backed key-value store, memory-mapping a fixed reservation.
#[derive(Debug)]
pub struct FileKv<K, V, S, A>
where
    A: Allocator,
{
    /// The backing file, kept alive because the mapping borrows its descriptor.
    _file: File,
    /// The single memory-mapped region holding every stored record.
    _map: MmapMut,
    /// The cached base address of the mapping; never remapped, so stable.
    base: *mut u8,
    /// The byte length of the mapping (the fixed reservation).
    capacity: usize,
    /// The byte cursor of the next record append.
    cursor: RefCell<usize>,
    /// The index mapping each key to the byte offset of its latest record.
    index: RefCell<HashMap<K, usize, S, A>>,
    /// Marks the record type the byte mapping is interpreted as.
    marker: PhantomData<(K, V)>,
}

impl<K, V, S, A> FileKv<K, V, S, A>
where
    K: Copy + Eq + Hash,
    V: Copy,
    S: BuildHasher,
    A: Allocator,
{
    /// Read the value stored for `key` under the index borrow.
    async fn read(&self, key: K) -> Option<V> {
        self.index.borrow().get(&key).copied().map(|offset| {
            // SAFETY: `base` is a stable, aligned pointer into the file-backed
            // mapping, and `offset` is a byte offset recorded by `write` that
            // lies within it.
            let ptr = unsafe { self.base.add(offset) }.cast::<(K, V)>();
            // SAFETY: `ptr` points at an initialized, correctly-aligned `(K, V)`
            // record (written by `write` and never mutated in place), so reading
            // it does not alias any live mutable reference.
            let record = unsafe { ptr.read() };
            record.1
        })
    }

    /// Append `(key, value)` and repoint `key` at its new record.
    async fn write(&self, key: K, value: V) -> Result<(), WriteError> {
        let record_size = size_of::<(K, V)>();
        let mut index = self.index.borrow_mut();
        index.try_reserve(1)?;
        let mut cursor = self.cursor.borrow_mut();
        let offset = *cursor;
        let next = offset
            .checked_add(record_size)
            .ok_or(WriteError::Overflow)?;
        if next > self.capacity {
            return Err(WriteError::Capacity);
        }
        let record = (key, value);
        // SAFETY: `base` is a stable, aligned pointer into the file-backed
        // mapping, and `offset` is the unpublished tail.
        let dst = unsafe { self.base.add(offset) };
        let src = core::ptr::from_ref(&record).cast::<u8>();
        // SAFETY: `src` points at the local `record` and `dst` points at the
        // unpublished tail `[offset, next)` of the mapping, which lies within
        // the reservation (checked above) and is aligned for `(K, V)`; the
        // destination is the next free slot, disjoint from every stored record.
        unsafe {
            core::ptr::copy_nonoverlapping(src, dst, record_size);
        }
        let _previous: Option<usize> = index.insert(key, offset);
        *cursor = next;
        Ok(())
    }
}

impl<K, V, S, A> FileKv<K, V, S, A>
where
    K: Copy + Eq + Hash + Ord,
    V: Copy,
    S: BuildHasher,
    A: Allocator,
{
    /// Return the smallest stored key strictly greater than `after`, if any.
    async fn next_key_inner(&self, after: Option<K>) -> Option<K> {
        self.index
            .borrow()
            .keys()
            .filter(|key| after.as_ref().is_none_or(|after_key| *key > after_key))
            .min()
            .copied()
    }
}

impl<K, V, S, A> TryFrom<Parts<S, A>> for FileKv<K, V, S, A>
where
    A: Allocator,
    S: BuildHasher,
{
    type Error = MapError;

    #[inline]
    fn try_from(parts: Parts<S, A>) -> Result<Self, Self::Error> {
        let Parts {
            alloc,
            capacity,
            file,
            hasher,
        } = parts;
        let align = align_of::<(K, V)>();
        if align > MIN_PAGE_ALIGN {
            return Err(MapError::Unaligned { align });
        }
        // SAFETY: `file.as_raw_fd()` names an open file that the caller has
        // sized to exactly `capacity` bytes and will not resize while the store
        // is live; the store keeps `file` alive for the mapping's lifetime, so
        // the descriptor stays valid and the mapped pages stay file-backed.
        let mut map = unsafe { MmapOptions::new().len(capacity).map_mut(file.as_raw_fd())? };
        let base = map.as_mut_ptr();
        Ok(Self {
            _file: file,
            _map: map,
            base,
            capacity,
            cursor: RefCell::new(0),
            index: RefCell::new(HashMap::with_hasher_in(hasher, alloc)),
            marker: PhantomData,
        })
    }
}

impl<K, V, S, A> Kv<K, V> for FileKv<K, V, S, A>
where
    K: Copy + Eq + Hash + Ord,
    V: Copy,
    S: BuildHasher,
    A: Allocator,
{
    type KeysError = EnumerateError;
    type LoadError = FetchError;
    type StoreError = WriteError;

    #[inline]
    async fn load(&self, key: K, cancel: &CancellationToken) -> Result<Option<V>, Self::LoadError> {
        cancel
            .run_until_cancelled(self.read(key))
            .await
            .ok_or(FetchError::Cancelled)
    }

    #[inline]
    async fn next_key(
        &self,
        after: Option<K>,
        cancel: &CancellationToken,
    ) -> Result<Option<K>, Self::KeysError> {
        cancel
            .run_until_cancelled(self.next_key_inner(after))
            .await
            .ok_or(EnumerateError::Cancelled)
    }

    #[inline]
    async fn store(
        &self,
        key: K,
        value: V,
        cancel: &CancellationToken,
    ) -> Result<(), Self::StoreError> {
        cancel
            .run_until_cancelled(self.write(key, value))
            .await
            .ok_or(WriteError::Cancelled)?
    }
}

/// Construction input for [`FileKv`].
#[derive(Debug)]
pub struct Parts<S, A> {
    /// The allocator the index is allocated in.
    pub alloc: A,
    /// The byte length of the reservation, equal to the file's length.
    pub capacity: usize,
    /// The already-opened backing file.
    ///
    /// The caller must have opened it read-write and extended it to exactly
    /// `capacity` bytes with [`File::set_len`] before construction, and must
    /// not resize it while the store is live.
    pub file: File,
    /// The hasher builder keys are hashed with.
    pub hasher: S,
}
