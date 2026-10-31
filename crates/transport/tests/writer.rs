//! Account-balance writer contract tests.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use rust_decimal::Decimal;
    use service::{AccountBalance, Balance, ClientId, Parts};
    use std::str::FromStr as _;
    use tokio_util::sync::CancellationToken;
    use transport::{WriteError, Writer};

    /// Build an account balance row for an input to the writer.
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

    /// Drive the writer over `balances`, returning the exact emitted bytes as a
    /// string.
    async fn write_all(balances: &[AccountBalance]) -> anyhow::Result<String> {
        let token = CancellationToken::new();
        let mut writer = Writer::from(Vec::from([]));
        writer.write_all(balances, &token).await?;
        Ok(String::from_utf8(writer.into_inner())?)
    }

    #[rudzio::test]
    async fn header_only_emits_header(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let output = write_all(&[]).await?;
        anyhow::ensure!(
            output == "client,available,held,total,locked\n",
            "unexpected header-only output: {output:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn spec_example_emits_shortest_decimals(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let balances = [
            balance(1, "1.5", "0.0", false)?,
            balance(2, "2.0", "0.0", false)?,
        ];
        let output = write_all(&balances).await?;
        anyhow::ensure!(
            output == "client,available,held,total,locked\n1,1.5,0,1.5,false\n2,2,0,2,false\n",
            "unexpected output: {output:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn trailing_zeros_are_normalized(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let balances = [balance(1, "2.0000", "1.0000", true)?];
        let output = write_all(&balances).await?;
        anyhow::ensure!(
            output == "client,available,held,total,locked\n1,2,1,3,true\n",
            "unexpected output: {output:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn cancellation_yields_typed_error(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        token.cancel();
        let mut writer = Writer::from(Vec::from([]));
        let result = writer
            .write_all(&[balance(1, "1.5", "0.0", false)?], &token)
            .await;
        anyhow::ensure!(
            matches!(result, Err(WriteError::Cancelled)),
            "expected cancelled, got {result:?}"
        );
        Ok(())
    }
}
