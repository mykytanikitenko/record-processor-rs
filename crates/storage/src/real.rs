//! The production storage backend; every method reports `Unimplemented` until
//! the mmap-backed store lands.

use record_processor_domain::client_id::ClientId;
use record_processor_domain::input::{Transaction, TxId};
use record_processor_domain::output::AccountBalance;
use tokio_util::sync::CancellationToken;

use crate::dedup::{Index, IndexInsertError, Insertion};
use crate::eof::{EndOfInput, SignalNotifyError, SignalWaitError};
use crate::event_queue::{EventQueue, IdRange, QueueAppendError, QueueReadError};
use crate::kv_state::{KvEntries, KvState, StateReadError, StateWriteError};

/// The production storage backend.
#[derive(Clone, Copy, Debug)]
pub struct Storage;

impl EventQueue for Storage {
    type AppendBatchError = QueueAppendError;
    type ReadBatchError = QueueReadError;

    #[inline]
    async fn append_batch(
        &mut self,
        _events: &[Transaction],
        _cancel: &CancellationToken,
    ) -> Result<IdRange, QueueAppendError> {
        Err(QueueAppendError::Unimplemented)
    }

    #[inline]
    async fn read_batch(
        &mut self,
        _from: u64,
        _events: &mut [Transaction],
        _cancel: &CancellationToken,
    ) -> Result<usize, QueueReadError> {
        Err(QueueReadError::Unimplemented)
    }
}

impl Index for Storage {
    type InsertIdError = IndexInsertError;

    #[inline]
    async fn insert(
        &mut self,
        _tx: TxId,
        _cancel: &CancellationToken,
    ) -> Result<Insertion, IndexInsertError> {
        Err(IndexInsertError::Unimplemented)
    }
}

impl EndOfInput for Storage {
    type NotifyEndError = SignalNotifyError;
    type WaitEndError = SignalWaitError;

    #[inline]
    async fn notify(
        &mut self,
        _end: u64,
        _cancel: &CancellationToken,
    ) -> Result<(), SignalNotifyError> {
        Err(SignalNotifyError::Unimplemented)
    }

    #[inline]
    async fn wait(&mut self, _cancel: &CancellationToken) -> Result<u64, SignalWaitError> {
        Err(SignalWaitError::Unimplemented)
    }
}

impl KvState for Storage {
    type ReadStateError = StateReadError;
    type WriteStateError = StateWriteError;

    #[inline]
    fn entries(&mut self) -> KvEntries {
        KvEntries::from(())
    }

    #[inline]
    async fn read(
        &mut self,
        _client: ClientId,
        _cancel: &CancellationToken,
    ) -> Result<Option<AccountBalance>, StateReadError> {
        Err(StateReadError::Unimplemented)
    }

    #[inline]
    async fn write(
        &mut self,
        _client: ClientId,
        _balance: &AccountBalance,
        _cancel: &CancellationToken,
    ) -> Result<(), StateWriteError> {
        Err(StateWriteError::Unimplemented)
    }
}
