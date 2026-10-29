//! Engine unit tests over an in-memory transaction source.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use domain::client_id::ClientId;
    use domain::input::{Amount, Transaction, TxId};
    use domain::output::{AccountBalance, Balance, Parts};
    use rust_decimal::Decimal;
    use service::Service as _;
    use service::{Engine, TransactionStream};
    use std::str::FromStr as _;

    /// An in-memory transaction stream: yields every transaction, then `Eof`,
    /// then terminates.
    struct TransactionSource {
        /// The remaining transactions to yield.
        items: std::vec::IntoIter<Transaction>,
    }

    impl TransactionSource {
        /// Build a source that yields `items` and then a terminal `Eof`.
        fn from(mut items: Vec<Transaction>) -> Self {
            items.push(Transaction::Eof);
            Self {
                items: items.into_iter(),
            }
        }
    }

    impl TransactionStream for TransactionSource {
        type Error = std::io::Error;

        async fn next(&mut self) -> Result<Option<Transaction>, std::io::Error> {
            Ok(self.items.next())
        }
    }

    /// Build a deposit transaction for the given client, tx, and amount.
    fn deposit(client: u16, tx: u32, amount: &str) -> anyhow::Result<Transaction> {
        Ok(Transaction::Deposit {
            client: ClientId::from(client),
            tx: TxId::from(tx),
            amount: Amount::try_from(Decimal::from_str(amount)?)?,
        })
    }

    /// Build a withdrawal transaction for the given client, tx, and amount.
    fn withdrawal(client: u16, tx: u32, amount: &str) -> anyhow::Result<Transaction> {
        Ok(Transaction::Withdrawal {
            client: ClientId::from(client),
            tx: TxId::from(tx),
            amount: Amount::try_from(Decimal::from_str(amount)?)?,
        })
    }

    /// Build a dispute transaction for the given client and tx.
    fn dispute(client: u16, tx: u32) -> Transaction {
        Transaction::Dispute {
            client: ClientId::from(client),
            tx: TxId::from(tx),
        }
    }

    /// Build a resolve transaction for the given client and tx.
    fn resolve(client: u16, tx: u32) -> Transaction {
        Transaction::Resolve {
            client: ClientId::from(client),
            tx: TxId::from(tx),
        }
    }

    /// Build a chargeback transaction for the given client and tx.
    fn chargeback(client: u16, tx: u32) -> Transaction {
        Transaction::Chargeback {
            client: ClientId::from(client),
            tx: TxId::from(tx),
        }
    }

    /// Build an expected account balance row.
    fn balance(
        client: u16,
        available: &str,
        held: &str,
        locked: bool,
    ) -> anyhow::Result<AccountBalance> {
        Ok(AccountBalance::try_from(Parts {
            client: ClientId::from(client),
            available: Balance::from(Decimal::from_str(available)?),
            held: Balance::from(Decimal::from_str(held)?),
            locked,
        })?)
    }

    /// Run the engine over `transactions` and return the resulting balances.
    async fn serve(transactions: Vec<Transaction>) -> anyhow::Result<Vec<AccountBalance>> {
        let engine = Engine::from(());
        Ok(engine.serve(TransactionSource::from(transactions)).await?)
    }

    #[rudzio::test]
    async fn deposit_and_withdrawal_accumulate(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let rows = serve(vec![
            deposit(1, 1, "1.0")?,
            deposit(2, 2, "2.0")?,
            deposit(1, 3, "2.0")?,
            withdrawal(1, 4, "1.5")?,
            withdrawal(2, 5, "3.0")?,
        ])
        .await?;
        anyhow::ensure!(
            rows == vec![
                balance(1, "1.5", "0.0", false)?,
                balance(2, "2.0", "0.0", false)?
            ],
            "unexpected balances: {rows:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn withdrawal_insufficient_is_ignored(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let rows = serve(vec![deposit(1, 1, "10.0")?, withdrawal(1, 2, "20.0")?]).await?;
        anyhow::ensure!(
            rows == vec![balance(1, "10.0", "0.0", false)?],
            "unexpected balances: {rows:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn dispute_moves_available_to_held(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let rows = serve(vec![deposit(1, 1, "10.0")?, dispute(1, 1)]).await?;
        anyhow::ensure!(
            rows == vec![balance(1, "0.0", "10.0", false)?],
            "unexpected balances: {rows:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn resolve_releases_held_to_available(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let rows = serve(vec![deposit(1, 1, "10.0")?, dispute(1, 1), resolve(1, 1)]).await?;
        anyhow::ensure!(
            rows == vec![balance(1, "10.0", "0.0", false)?],
            "unexpected balances: {rows:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn chargeback_drains_held_and_locks(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let rows = serve(vec![
            deposit(1, 1, "10.0")?,
            dispute(1, 1),
            chargeback(1, 1),
        ])
        .await?;
        anyhow::ensure!(
            rows == vec![balance(1, "0.0", "0.0", true)?],
            "unexpected balances: {rows:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn dispute_unknown_tx_is_ignored(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let rows = serve(vec![deposit(1, 1, "10.0")?, dispute(1, 999)]).await?;
        anyhow::ensure!(
            rows == vec![balance(1, "10.0", "0.0", false)?],
            "unexpected balances: {rows:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn resolve_and_chargeback_without_dispute_ignored(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let rows = serve(vec![
            deposit(1, 1, "10.0")?,
            resolve(1, 1),
            chargeback(1, 1),
        ])
        .await?;
        anyhow::ensure!(
            rows == vec![balance(1, "10.0", "0.0", false)?],
            "unexpected balances: {rows:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn empty_stream_produces_no_balances(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let rows = serve(Vec::new()).await?;
        anyhow::ensure!(rows.is_empty(), "expected no balances, got {rows:?}");
        Ok(())
    }
}
