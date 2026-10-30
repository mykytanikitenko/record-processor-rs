//! Account-balance writer contract tests.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use record_processor_domain::client_id::ClientId;
    use record_processor_domain::output::{AccountBalance, Balance, Parts};
    use rust_decimal::Decimal;
    use std::str::FromStr as _;
    use tokio_util::sync::CancellationToken;
    use transport::{
        MockWriteError, MockWriter, WriteDispatchError, WriteError, Writer, WriterType,
    };

    /// Build a single account balance row.
    fn balance() -> anyhow::Result<AccountBalance> {
        Ok(AccountBalance::try_from(Parts {
            client: ClientId::from(1),
            available: Balance::from(Decimal::from_str("1.5")?),
            held: Balance::from(Decimal::from_str("0.0")?),
            locked: false,
        })?)
    }

    #[rudzio::test]
    async fn real_writer_is_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut writer = Writer::from(compio::fs::stdout());
        let result = writer.write_all(&[balance()?], &token).await;
        anyhow::ensure!(
            result == Err(WriteError::Unimplemented),
            "expected Unimplemented, got {result:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn real_writer_yields_cancelled(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        token.cancel();
        let mut writer = Writer::from(compio::fs::stdout());
        let result = writer.write_all(&[balance()?], &token).await;
        anyhow::ensure!(
            result == Err(WriteError::Cancelled),
            "expected Cancelled, got {result:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn mock_writer_reports_success(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut writer = MockWriter;
        let result = writer.write_all(&[balance()?], &token).await;
        anyhow::ensure!(result == Ok(()), "expected Ok, got {result:?}");
        Ok(())
    }

    #[rudzio::test]
    async fn mock_writer_yields_cancelled(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        token.cancel();
        let mut writer = MockWriter;
        let result = writer.write_all(&[balance()?], &token).await;
        anyhow::ensure!(
            result == Err(MockWriteError::Cancelled),
            "expected Cancelled, got {result:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn writer_type_dispatches_real(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut writer = WriterType::Real(Writer::from(compio::fs::stdout()));
        let result = writer.write_all(&[balance()?], &token).await;
        anyhow::ensure!(
            result == Err(WriteDispatchError::Real(WriteError::Unimplemented)),
            "expected Unimplemented, got {result:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn writer_type_dispatches_mock(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut writer: WriterType<compio::fs::Stdout> = WriterType::Mock(MockWriter);
        let result = writer.write_all(&[balance()?], &token).await;
        anyhow::ensure!(result == Ok(()), "expected Ok, got {result:?}");
        Ok(())
    }
}
