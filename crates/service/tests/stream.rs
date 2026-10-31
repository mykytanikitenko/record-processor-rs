//! Slice-stream tests over hand-written batch sources.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use record_processor_domain::client_id::ClientId;
    use record_processor_domain::input::{Amount, Transaction, TxId};
    use record_processor_domain::output::{AccountBalance, Balance, Parts};
    use rust_decimal::Decimal;
    use service::{Balances as _, Transactions as _};
    use std::convert::Infallible;
    use std::str::FromStr as _;
    use tokio_util::sync::CancellationToken;

    /// A transaction batch source that yields its rows once.
    struct TransactionSource {
        /// The rows yielded by [`TransactionSource::next_batch`].
        rows: Vec<Transaction>,
    }

    impl service::Transactions for TransactionSource {
        type Error = Infallible;

        async fn next_batch<'stream>(
            &'stream self,
            _cancel: &CancellationToken,
        ) -> Result<Option<&'stream [Transaction]>, Self::Error> {
            Ok(Some(&self.rows))
        }
    }

    /// A balance batch source that yields its rows once.
    struct BalanceSource {
        /// The rows yielded by [`BalanceSource::next_batch`].
        rows: Vec<AccountBalance>,
    }

    impl service::Balances for BalanceSource {
        type Error = Infallible;

        async fn next_batch<'stream>(
            &'stream mut self,
            _cancel: &CancellationToken,
        ) -> Result<Option<&'stream [AccountBalance]>, Self::Error> {
            Ok(Some(&self.rows))
        }
    }

    /// Build a deposit transaction.
    fn deposit(client: u16, tx: u32, amount: &str) -> anyhow::Result<Transaction> {
        Ok(Transaction::Deposit {
            client: ClientId::from(client),
            tx: TxId::from(tx),
            amount: Amount::try_from(Decimal::from_str(amount)?)?,
        })
    }

    /// Build a balance row.
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

    #[rudzio::test]
    async fn transactions_yield_the_appended_batch(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let source = TransactionSource {
            rows: vec![deposit(1, 1, "1.5")?, deposit(2, 2, "2.0")?],
        };
        let expected = source.rows.as_slice();
        let batch = source.next_batch(&CancellationToken::new()).await;
        anyhow::ensure!(
            matches!(batch, Ok(Some(rows)) if rows == expected),
            "unexpected batch {batch:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn balances_yield_the_appended_batch(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let expected = vec![
            balance(1, "1.5", "0.0", false)?,
            balance(2, "0.0", "2.0", true)?,
        ];
        let mut source = BalanceSource {
            rows: expected.clone(),
        };
        let batch = source.next_batch(&CancellationToken::new()).await;
        anyhow::ensure!(
            matches!(batch, Ok(Some(rows)) if rows == expected.as_slice()),
            "unexpected batch {batch:?}"
        );
        Ok(())
    }
}
