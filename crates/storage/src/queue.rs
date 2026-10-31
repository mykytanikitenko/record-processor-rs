//! The append-only queue contract: unbounded appends of fixed-size records,
//! addressed by record position.

/// A record position in the queue: the zero-based index of a record since the
/// start of the queue. A memory-mapped backing derives the byte offset as
/// `position * size_of::<T>()`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, derive_more::Deref)]
pub struct Position(u64);

impl From<u64> for Position {
    #[inline]
    fn from(index: u64) -> Self {
        Self(index)
    }
}

/// A half-open range of record positions `[start, end)`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Span {
    /// One past the last record of the range.
    pub end: Position,
    /// The first record of the range.
    pub start: Position,
}

/// Appending batches of fixed-size `T` records after the queue tail.
///
/// Takes `&self` so a producer and readers share the queue by reference; a
/// backing serialises appends internally.
pub trait Append<T>
where
    T: Copy,
{
    /// The error produced by [`Append::append`].
    type Error: core::error::Error;

    /// Append every record of `batch` after the current tail, returning the
    /// span the batch now occupies. An empty batch yields an empty span at the
    /// tail.
    async fn append(&self, batch: &[T]) -> Result<Span, Self::Error>;
}

/// Reading fixed-size `T` records back from a position.
pub trait ReadAt<T>
where
    T: Copy,
{
    /// The error produced by [`ReadAt::read`].
    type Error: core::error::Error;

    /// Copy records starting at `from` into `into`, returning the span copied.
    /// The span is shorter than `into` (possibly empty) when `from` is near the
    /// tail.
    async fn read(&self, from: Position, into: &mut [T]) -> Result<Span, Self::Error>;
}

/// An append-only, unbounded queue of fixed-size `T` records: every type that
/// can both [`Append`] and [`ReadAt`].
pub trait Queue<T>: Append<T> + ReadAt<T>
where
    T: Copy,
{
}

impl<T, Q> Queue<T> for Q
where
    T: Copy,
    Q: Append<T> + ReadAt<T>,
{
}
