//! Memory-mapped key-value integration tests against the real backing.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use record_processor_domain::client_id::ClientId;
    use record_processor_domain::output::{AccountBalance, Balance, Parts};
    use rust_decimal::Decimal;
    use storage::mmap_kv::{LookupError, UpdateError, WalkError};
    use storage::{Lookup, MmapKv, Slot, Update as _, Walk};

    /// Build a client balance to store.
    fn balance() -> anyhow::Result<AccountBalance> {
        Ok(AccountBalance::try_from(Parts {
            client: ClientId::from(1),
            available: Balance::from(Decimal::ONE),
            held: Balance::from(Decimal::ZERO),
            locked: false,
        })?)
    }

    #[rudzio::test]
    async fn update_is_unimplemented(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let kv = MmapKv;
        let updated = kv.update(ClientId::from(1), &balance()?).await;
        anyhow::ensure!(
            updated == Err(UpdateError::Unimplemented),
            "expected Unimplemented, got {updated:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn lookup_is_unimplemented(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let kv = MmapKv;
        let read = Lookup::<ClientId, AccountBalance>::read(&kv, ClientId::from(1)).await;
        anyhow::ensure!(
            read == Err(LookupError::Unimplemented),
            "expected Unimplemented, got {read:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn walk_is_unimplemented(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let kv = MmapKv;
        let entry = Walk::<ClientId, AccountBalance>::next_entry(&kv, Slot::START).await;
        anyhow::ensure!(
            entry == Err(WalkError::Unimplemented),
            "expected Unimplemented, got {entry:?}"
        );
        Ok(())
    }
}
