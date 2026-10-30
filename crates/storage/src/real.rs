//! The production storage backend (a contract stub until the mmap store
//! lands).

use record_processor_domain::client_id::ClientId;
use record_processor_domain::input::{Transaction, TxId};
use record_processor_domain::output::AccountBalance;

use crate::dedup::{self, Index};
use crate::eof::{self, EndOfInput};
use crate::event_queue::{self, EventQueue};
use crate::kv_state::{self, KvEntries, KvState};

/// The production storage backend. Every method is a contract stub returning
/// the module's `Unimplemented` error.
#[derive(Clone, Copy, Debug)]
pub struct Storage;

impl EventQueue for Storage {
    type Error = event_queue::QueueError;

    #[inline]
    async fn append(&mut self, _event: &Transaction) -> Result<u32, Self::Error> {
        Err(event_queue::QueueError::Unimplemented)
    }

    #[inline]
    async fn read_batch(&mut self, _batch: &mut [u8]) -> Result<usize, Self::Error> {
        Err(event_queue::QueueError::Unimplemented)
    }
}

impl Index for Storage {
    type Error = dedup::IndexError;

    #[inline]
    async fn insert(&mut self, _tx: &TxId) -> Result<(), Self::Error> {
        Err(dedup::IndexError::Unimplemented)
    }
}

impl KvState for Storage {
    type Error = kv_state::StateError;

    #[inline]
    fn entries(&mut self) -> KvEntries {
        KvEntries::from(())
    }

    #[inline]
    async fn update(
        &mut self,
        _client: ClientId,
        _balance: &AccountBalance,
    ) -> Result<(), Self::Error> {
        Err(kv_state::StateError::Unimplemented)
    }
}

impl EndOfInput for Storage {
    type Error = eof::EndOfInputError;

    #[inline]
    async fn notify(&mut self) -> Result<(), Self::Error> {
        Err(eof::EndOfInputError::Unimplemented)
    }

    #[inline]
    async fn wait(&self) -> Result<(), Self::Error> {
        Err(eof::EndOfInputError::Unimplemented)
    }
}
