//! The payments engine: business logic over borrowed storage.

/// The payments engine, generic over the storage contracts it borrows.
///
/// `Q` is the transaction queue and `K` the per-client state. Each stage bounds
/// only the storage operations it uses: [`Engine::ingest`] needs
/// [`storage::Append`], [`Engine::aggregate`] needs [`storage::ReadAt`],
/// [`storage::Lookup`] and [`storage::Update`], and [`Engine::balances`]
/// needs [`storage::Walk`]. The engine never names a concrete backing; the
/// composition root lends both stores.
///
/// The pipeline runs in three stages: [`Engine::ingest`] appends the input
/// stream to the queue in batches, [`Engine::aggregate`] reads the queue and
/// applies it to per-client state through tracked worker jobs, and
/// [`Engine::balances`] streams the final state lazily.
#[derive(Clone, Copy, Debug)]
pub struct Engine<'store, Q, K> {
    /// The per-client state store.
    pub kv: &'store K,
    /// The transaction queue.
    pub queue: &'store Q,
}
