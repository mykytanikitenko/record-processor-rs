//! Storage contracts: an append-only record queue and a key-value state store.

pub mod kv;
pub mod mmap_kv;
pub mod mmap_queue;
pub mod queue;

// TODO(lint/E0432): unresolved imports `kv::Entry`, `kv::Lookup`, `kv::Slot`, `kv::Update`, `kv::Walk`
pub use kv::{Entry, Kv, Lookup, Slot, Update, Walk};
pub use mmap_kv::MmapKv;
pub use mmap_queue::MmapQueue;
// TODO(lint/E0432): unresolved imports `queue::Append`, `queue::Position`, `queue::ReadAt`, `queue::Span`
pub use queue::{Append, Position, Queue, ReadAt, Span};
