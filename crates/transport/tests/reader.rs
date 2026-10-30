//! Transaction reader contract tests.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use compio::buf::{BufResult, IoBufMut};
    use compio::io::AsyncRead;
    use record_processor_domain::client_id::ClientId;
    use record_processor_domain::input::{Amount, ConstructionError, Transaction, TxId};
    use rust_decimal::Decimal;
    use service::TransactionStream as _;
    use std::str::FromStr as _;
    use tokio_util::sync::CancellationToken;
    use transport::{Input, ReadError, Reader, RecordError};

    /// A byte source that yields at most `chunk` bytes per read, forcing the
    /// reader to reassemble records across buffer refills.
    struct Chunked<'bytes> {
        /// The remaining bytes to yield.
        bytes: &'bytes [u8],
        /// The maximum number of bytes yielded per read.
        chunk: usize,
    }

    impl AsyncRead for Chunked<'_> {
        async fn read<B>(&mut self, mut buf: B) -> BufResult<usize, B>
        where
            B: IoBufMut,
        {
            let n = self.chunk.min(self.bytes.len());
            let (chunk, rest) = self.bytes.split_at(n);
            match buf.extend_from_slice(chunk) {
                Ok(()) => {}
                Err(error) => return BufResult(Err(error.into()), buf),
            }
            self.bytes = rest;
            BufResult(Ok(n), buf)
        }
    }

    /// Build a deposit transaction for an expected-value assertion.
    fn deposit(client: u16, tx: u32, amount: &str) -> anyhow::Result<Transaction> {
        Ok(Transaction::Deposit {
            client: ClientId::from(client),
            tx: TxId::from(tx),
            amount: Amount::try_from(Decimal::from_str(amount)?)?,
        })
    }

    /// Drive a reader over `input` until the first error is produced.
    async fn first_error(input: &[u8]) -> anyhow::Result<ReadError> {
        let token = CancellationToken::new();
        let mut reader = Reader::from(Input::Stream(input));
        match reader.next(&token).await {
            Err(error) => Ok(error),
            Ok(item) => anyhow::bail!("expected an error, got {item:?}"),
        }
    }

    /// Drive a reader over `source` to completion, collecting every
    /// transaction and dropping the trailing end-of-input marker.
    async fn read_all(source: impl AsyncRead) -> anyhow::Result<Vec<Transaction>> {
        let token = CancellationToken::new();
        let mut reader = Reader::from(Input::Stream(source));
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
    async fn header_only_yields_eof_then_none(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut reader = Reader::from(Input::Stream(b"type,client,tx,amount\n".as_slice()));
        anyhow::ensure!(
            matches!(reader.next(&token).await, Ok(Some(Transaction::Eof))),
            "expected the end-of-input marker"
        );
        anyhow::ensure!(
            matches!(reader.next(&token).await, Ok(None)),
            "expected termination after the end-of-input marker"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn multiple_deposits(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let input = b"type,client,tx,amount\ndeposit,1,1,1.0\ndeposit,2,2,2.5\n";
        let transactions = read_all(input.as_slice()).await?;
        anyhow::ensure!(
            transactions == [deposit(1, 1, "1.0")?, deposit(2, 2, "2.5")?],
            "unexpected transactions: {transactions:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn crlf_line_endings(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let input = b"type,client,tx,amount\r\ndeposit,1,1,1.0\r\ndeposit,2,2,2.5\r\n";
        let transactions = read_all(input.as_slice()).await?;
        anyhow::ensure!(
            transactions == [deposit(1, 1, "1.0")?, deposit(2, 2, "2.5")?],
            "unexpected transactions: {transactions:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn whitespace_is_trimmed(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let input = b"type, client, tx, amount\n  deposit  , 1 , 1 ,  1.0\n";
        let transactions = read_all(input.as_slice()).await?;
        anyhow::ensure!(
            transactions == [deposit(1, 1, "1.0")?],
            "unexpected transactions: {transactions:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn record_split_across_refills(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let input = b"type,client,tx,amount\ndeposit,1,1,1.0\ndeposit,2,2,2.5\n";
        let source = Chunked {
            bytes: input.as_slice(),
            chunk: 3,
        };
        let transactions = read_all(source).await?;
        anyhow::ensure!(
            transactions == [deposit(1, 1, "1.0")?, deposit(2, 2, "2.5")?],
            "unexpected transactions: {transactions:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn cancellation_yields_typed_error(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        token.cancel();
        let mut reader = Reader::from(Input::Stream(b"type,client,tx,amount\n".as_slice()));
        anyhow::ensure!(
            matches!(reader.next(&token).await, Err(ReadError::Cancelled)),
            "expected a cancelled error"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn unknown_type_is_rejected(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let error = first_error(b"type,client,tx,amount\nmystery,1,1,1.0\n").await?;
        anyhow::ensure!(
            matches!(error, ReadError::Record(RecordError::UnknownType)),
            "expected unknown-type, got {error:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn literal_eof_is_rejected(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let error = first_error(b"type,client,tx,amount\neof,1,1,1.0\n").await?;
        anyhow::ensure!(
            matches!(error, ReadError::Record(RecordError::UnknownType)),
            "expected unknown-type for a literal `eof`, got {error:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn amount_on_dispute_is_rejected(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let error = first_error(b"type,client,tx,amount\ndispute,1,1,5.0\n").await?;
        anyhow::ensure!(
            matches!(error, ReadError::Record(RecordError::UnexpectedAmount)),
            "expected unexpected-amount, got {error:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn missing_amount_is_rejected(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let error = first_error(b"type,client,tx,amount\ndeposit,1,1\n").await?;
        anyhow::ensure!(
            matches!(error, ReadError::Record(RecordError::MissingAmount)),
            "expected missing-amount, got {error:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn negative_amount_is_rejected(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let error = first_error(b"type,client,tx,amount\ndeposit,1,1,-5.0\n").await?;
        anyhow::ensure!(
            matches!(
                error,
                ReadError::Record(RecordError::Amount(ConstructionError::NonPositive { .. }))
            ),
            "expected non-positive amount, got {error:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn zero_amount_is_rejected(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let error = first_error(b"type,client,tx,amount\ndeposit,1,1,0.0\n").await?;
        anyhow::ensure!(
            matches!(
                error,
                ReadError::Record(RecordError::Amount(ConstructionError::NonPositive { .. }))
            ),
            "expected non-positive amount, got {error:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn over_4dp_amount_is_rejected(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let error = first_error(b"type,client,tx,amount\ndeposit,1,1,1.23456\n").await?;
        anyhow::ensure!(
            matches!(
                error,
                ReadError::Record(RecordError::Amount(
                    ConstructionError::TooManyDecimalPlaces { .. }
                ))
            ),
            "expected too-many-decimal-places, got {error:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn non_numeric_client_is_rejected(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let error = first_error(b"type,client,tx,amount\ndeposit,abc,1,1.0\n").await?;
        anyhow::ensure!(
            matches!(error, ReadError::Record(RecordError::Client(_))),
            "expected invalid-client, got {error:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn non_numeric_tx_is_rejected(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let error = first_error(b"type,client,tx,amount\ndeposit,1,xyz,1.0\n").await?;
        anyhow::ensure!(
            matches!(error, ReadError::Record(RecordError::Tx(_))),
            "expected invalid-transaction, got {error:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn client_over_u16_is_rejected(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let error = first_error(b"type,client,tx,amount\ndeposit,65536,1,1.0\n").await?;
        anyhow::ensure!(
            matches!(error, ReadError::Record(RecordError::Client(_))),
            "expected invalid-client, got {error:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn tx_over_u32_is_rejected(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let error = first_error(b"type,client,tx,amount\ndeposit,1,4294967296,1.0\n").await?;
        anyhow::ensure!(
            matches!(error, ReadError::Record(RecordError::Tx(_))),
            "expected invalid-transaction, got {error:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn wrong_header_is_rejected(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let error = first_error(b"foo,bar,baz,qux\ndeposit,1,1,1.0\n").await?;
        anyhow::ensure!(
            matches!(error, ReadError::Record(RecordError::Header)),
            "expected header error, got {error:?}"
        );
        Ok(())
    }
}
