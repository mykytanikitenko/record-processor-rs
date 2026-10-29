//! Account balance writer tests over an in-memory compio sink.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use domain::client_id::ClientId;
    use domain::output::{AccountBalance, Balance, Parts};
    use rust_decimal::Decimal;
    use std::str::FromStr as _;
    use transport::AccountBalanceWriter;

    #[rudzio::test]
    fn writes_balances_to_the_sink(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        let balance = AccountBalance::try_from(Parts {
            client: ClientId::from(1),
            available: Balance::from(Decimal::from_str("1.5")?),
            held: Balance::from(Decimal::from_str("0.0")?),
            locked: false,
        })?;
        let mut writer = AccountBalanceWriter::from(Vec::<u8>::new());
        let runtime = compio::runtime::Runtime::new()?;
        runtime.block_on(writer.write_all(&[balance]))?;
        let bytes = writer.into_inner();
        let output = String::from_utf8(bytes)?;
        anyhow::ensure!(
            output == "client,available,held,total,locked\n1,1.5,0.0,1.5,false\n",
            "unexpected output: {output:?}"
        );
        Ok(())
    }
}
