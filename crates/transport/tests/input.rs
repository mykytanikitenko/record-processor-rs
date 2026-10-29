//! Transaction reader tests over an in-memory compio source.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use record_processor_domain::client_id::ClientId;
    use record_processor_domain::input::{Amount, Transaction, TxId};
    use rust_decimal::Decimal;
    use service::TransactionStream as _;
    use std::str::FromStr as _;
    use transport::TransactionReader;

    /// Drain `data` through the reader into a transaction vector.
    fn read(data: &[u8]) -> anyhow::Result<Vec<Transaction>> {
        let source = std::io::Cursor::new(data);
        let mut reader = TransactionReader::from(source);
        let runtime = compio::runtime::Runtime::new()?;
        runtime.block_on(async {
            let mut transactions = Vec::new();
            while let Some(transaction) = reader.next().await? {
                transactions.push(transaction);
            }
            Ok(transactions)
        })
    }

    /// Build a deposit transaction for the given client, tx, and amount.
    fn deposit(client: u16, tx: u32, amount: &str) -> anyhow::Result<Transaction> {
        Ok(Transaction::Deposit {
            client: ClientId::from(client),
            tx: TxId::from(tx),
            amount: Amount::try_from(Decimal::from_str(amount)?)?,
        })
    }

    #[rudzio::test]
    fn reads_transactions_and_emits_eof(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let data = b"type,client,tx,amount\ndeposit,1,1,1.0\nwithdrawal,1,2,0.5\n";
        let transactions = read(data)?;
        anyhow::ensure!(
            transactions
                == vec![
                    deposit(1, 1, "1.0")?,
                    Transaction::Withdrawal {
                        client: ClientId::from(1),
                        tx: TxId::from(2),
                        amount: Amount::try_from(Decimal::from_str("0.5")?)?,
                    },
                    Transaction::Eof,
                ],
            "unexpected transactions: {transactions:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    fn reads_dispute_row_without_amount(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let data = b"type,client,tx,amount\ndeposit,1,1,1.0\ndispute,1,1\n";
        let transactions = read(data)?;
        anyhow::ensure!(
            transactions
                == vec![
                    deposit(1, 1, "1.0")?,
                    Transaction::Dispute {
                        client: ClientId::from(1),
                        tx: TxId::from(1),
                    },
                    Transaction::Eof,
                ],
            "unexpected transactions: {transactions:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    fn header_only_emits_just_eof(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let data = b"type,client,tx,amount\n";
        let transactions = read(data)?;
        anyhow::ensure!(
            transactions == vec![Transaction::Eof],
            "unexpected transactions: {transactions:?}"
        );
        Ok(())
    }
}
