//! Storage contracts: an append-only record queue and a key-value state store.

pub mod kv;
pub mod mmap_kv;
pub mod mmap_queue;
pub mod queue;

pub use kv::{Entry, Kv, Lookup, Slot, Update, Walk};
pub use mmap_kv::MmapKv;
pub use mmap_queue::MmapQueue;
pub use queue::{Append, Position, Queue, ReadAt, Span};
