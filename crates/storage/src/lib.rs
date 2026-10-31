//! Storage contracts and their in-memory backings: an append-only record
//! queue, a key-value store, and the engine state composed over key-value
//! stores.

pub mod kv;
pub mod memory_kv;
pub mod memory_queue;
#[cfg(feature = "mocks")]
pub mod mocks;
pub mod offset;
pub mod queue;
pub mod state;

pub use kv::Kv;
pub use memory_kv::MemoryKv;
pub use memory_queue::MemoryQueue;
pub use offset::Offset;
pub use queue::Queue;
pub use state::{ClientState, CommittedOffset, KvState, State};
