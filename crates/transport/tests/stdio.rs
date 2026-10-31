//! Stdio adapter contract tests.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use rust_decimal::Decimal;
    use service::{Amount, ClientId, Transaction, Transactions as _, TxId};
    use std::str::FromStr as _;
    use tokio_util::sync::CancellationToken;
    use transport::{Input, Transactions};

    /// Build a deposit transaction for an expected-value assertion.
    fn deposit(client: u16, tx: u32, amount: &str) -> anyhow::Result<Transaction> {
        Ok(Transaction::Deposit {
            client: ClientId::from(client),
            tx: TxId::from(tx),
            amount: Amount::try_from(Decimal::from_str(amount)?)?,
        })
    }

    #[rudzio::test]
    async fn adapter_yields_transactions_then_none(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut adapter = Transactions::from(Input::Stream(
            b"type,client,tx,amount\ndeposit,1,1,1.0\ndeposit,2,2,2.5\n".as_slice(),
        ));

        let expected = [deposit(1, 1, "1.0")?, deposit(2, 2, "2.5")?];

        let Some(batch) = adapter.next_batch(&token).await? else {
            anyhow::bail!("expected a batch, got None");
        };
        anyhow::ensure!(batch == expected.as_slice(), "unexpected batch: {batch:?}");

        anyhow::ensure!(
            adapter.next_batch(&token).await?.is_none(),
            "expected None after exhaustion"
        );

        Ok(())
    }
}
