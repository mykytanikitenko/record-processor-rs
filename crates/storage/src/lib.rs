//! Storage for the record-processor engine: an append-only event queue, a
//! transaction-id deduplication index, an end-of-input signal and per-client
//! state. Persistence only; every business rule lives in the service crate.

pub mod dedup;
pub mod eof;
pub mod event_queue;
pub mod kv_state;
pub mod real;

pub use dedup::{Index as DedupIndex, IndexInsertError, Insertion};
pub use eof::{EndOfInput, SignalNotifyError, SignalWaitError};
pub use event_queue::{EventQueue, IdRange, QueueAppendError, QueueReadError};
pub use kv_state::{KvEntries, KvState, NextEntryError, StateReadError, StateWriteError};
pub use real::Storage;
