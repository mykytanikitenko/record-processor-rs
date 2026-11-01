//! Storage contracts and their in-memory backings: an append-only record
//! queue, a key-value store, and the engine state composed over key-value
//! stores.

pub mod engine_state;
pub mod file_kv;
pub mod file_queue;
pub mod kv;
pub mod memory_kv;
pub mod memory_queue;
#[cfg(feature = "mocks")]
pub mod mocks;
pub mod offset;
pub mod queue;

pub use engine_state::{ClientState, CommittedOffset, KvState, State};
pub use file_kv::FileKv;
pub use file_queue::FileQueue;
pub use kv::Kv;
pub use memory_kv::MemoryKv;
pub use memory_queue::MemoryQueue;
pub use offset::Offset;
pub use queue::Queue;
