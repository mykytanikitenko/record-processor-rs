//! Account and transaction state store contracts.

pub mod dedup;
pub mod eof;
pub mod event_queue;
pub mod kv_state;
pub mod real;

pub use dedup::{Index as DedupIndex, IndexError};
pub use eof::{EndOfInput, EndOfInputError};
pub use event_queue::{EventQueue, QueueError};
pub use kv_state::{KvEntries, KvState, NextEntryError, StateError};
pub use real::Storage;
