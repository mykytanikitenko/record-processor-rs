//! Transaction reader contract tests.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use record_processor_domain::input::Transaction;
    use service::TransactionStream as _;
    use tokio_util::sync::CancellationToken;
    use transport::{MockReadError, MockReader, ReadError, Reader, ReaderError, ReaderType};

    #[rudzio::test]
    async fn real_reader_is_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut reader = Reader::from(Vec::<u8>::new());
        let item = reader.next(&token).await;
        anyhow::ensure!(
            item == Err(ReadError::Unimplemented),
            "expected Unimplemented, got {item:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn real_reader_yields_cancelled(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        token.cancel();
        let mut reader = Reader::from(Vec::<u8>::new());
        let item = reader.next(&token).await;
        anyhow::ensure!(
            item == Err(ReadError::Cancelled),
            "expected Cancelled, got {item:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn mock_reader_yields_item_then_none(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut reader = MockReader::from(Some(Transaction::Eof));
        let first = reader.next(&token).await;
        anyhow::ensure!(
            first == Ok(Some(Transaction::Eof)),
            "expected Eof, got {first:?}"
        );
        let second = reader.next(&token).await;
        anyhow::ensure!(second == Ok(None), "expected end, got {second:?}");
        Ok(())
    }

    #[rudzio::test]
    async fn mock_reader_yields_cancelled(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        token.cancel();
        let mut reader = MockReader::from(None);
        let item = reader.next(&token).await;
        anyhow::ensure!(
            item == Err(MockReadError::Cancelled),
            "expected Cancelled, got {item:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn reader_type_dispatches_real(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut reader = ReaderType::Real(Reader::from(Vec::<u8>::new()));
        let item = reader.next(&token).await;
        anyhow::ensure!(
            item == Err(ReaderError::Real(ReadError::Unimplemented)),
            "expected Unimplemented, got {item:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn reader_type_dispatches_mock(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut reader: ReaderType<Vec<u8>> = ReaderType::Mock(MockReader::from(None));
        let item = reader.next(&token).await;
        anyhow::ensure!(item == Ok(None), "expected end, got {item:?}");
        Ok(())
    }
}
