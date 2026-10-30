//! Account and transaction state store contracts.

pub mod dedup;
pub mod eof;
pub mod event_queue;
pub mod kv_state;
pub mod real;

#[cfg(feature = "unit")]
pub use dedup::MockIndex as MockDedupIndex;
pub use dedup::{Index as DedupIndex, IndexError};
#[cfg(feature = "unit")]
pub use eof::MockEndOfInput;
pub use eof::{EndOfInput, EndOfInputError};
#[cfg(feature = "unit")]
pub use event_queue::MockEventQueue;
pub use event_queue::{EventQueue, QueueError};
#[cfg(feature = "unit")]
pub use kv_state::MockKvState;
pub use kv_state::{KvEntries, KvState, NextEntryError, StateError};
pub use real::Storage;
