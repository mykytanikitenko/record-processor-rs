//! File-backed transaction reader tests: the file is streamed through the
//! reader's single reused buffer, never read whole.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use compio::io::AsyncRead;
    use rust_decimal::Decimal;
    use service::{Amount, ClientId, Transaction, TxId};
    use std::str::FromStr as _;
    use tokio_util::sync::CancellationToken;
    use transport::{Input, Reader};

    /// Build a deposit transaction for an expected-value assertion.
    fn deposit(client: u16, tx: u32, amount: &str) -> anyhow::Result<Transaction> {
        Ok(Transaction::Deposit {
            client: ClientId::from(client),
            tx: TxId::from(tx),
            amount: Amount::try_from(Decimal::from_str(amount)?)?,
        })
    }

    /// Drive a reader to completion, dropping the end-of-input marker.
    async fn read_all<S>(mut reader: Reader<S>) -> anyhow::Result<Vec<Transaction>>
    where
        S: AsyncRead,
    {
        let token = CancellationToken::new();
        let mut transactions = Vec::new();
        loop {
            match reader.next(&token).await? {
                Some(Transaction::Eof) => {}
                Some(transaction) => transactions.push(transaction),
                None => return Ok(transactions),
            }
        }
    }

    #[rudzio::test]
    async fn file_larger_than_buffer_streams_across_refills(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        // 22-byte header + 16-byte rows: the third row straddles the first
        // 64-byte refill (bytes 54..70).
        let content = "type,client,tx,amount\n\
            deposit,1,1,1.0\n\
            deposit,2,2,2.5\n\
            deposit,3,3,3.5\n\
            deposit,4,4,4.5\n\
            deposit,5,5,5.5\n\
            deposit,6,6,6.5\n\
            deposit,7,7,7.5\n\
            deposit,8,8,8.5\n";
        let path =
            std::env::temp_dir().join(format!("record-processor-{}.csv", std::process::id()));
        std::fs::write(&path, content)?;
        let runtime = compio::runtime::Runtime::new()?;
        let transactions = runtime.block_on(async {
            let file = compio::fs::File::open(&path).await?;
            read_all(Reader::<compio::fs::Stdin>::from(Input::from(file))).await
        })?;
        std::fs::remove_file(&path)?;
        anyhow::ensure!(
            transactions
                == [
                    deposit(1, 1, "1.0")?,
                    deposit(2, 2, "2.5")?,
                    deposit(3, 3, "3.5")?,
                    deposit(4, 4, "4.5")?,
                    deposit(5, 5, "5.5")?,
                    deposit(6, 6, "6.5")?,
                    deposit(7, 7, "7.5")?,
                    deposit(8, 8, "8.5")?,
                ],
            "unexpected transactions: {transactions:?}"
        );
        Ok(())
    }
}
