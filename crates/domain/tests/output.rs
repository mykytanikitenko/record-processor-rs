//! Output-side domain tests: account balance serialization.

use rudzio::common::context::Suite;
use rudzio::runtime::compio::Runtime;

#[rudzio::suite([(runtime = Runtime::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use record_processor_domain::client_id::ClientId;
    use record_processor_domain::output::{AccountBalance, Balance, OverflowError, Parts};
    use rust_decimal::Decimal;
    use std::str::FromStr as _;

    #[rudzio::test]
    fn serializes_account_balance() -> anyhow::Result<()> {
        let row = AccountBalance::try_from(Parts {
            client: ClientId::from(1),
            available: Balance::from(Decimal::from_str("1.5")?),
            held: Balance::from(Decimal::from_str("0.0")?),
            locked: false,
        })?;
        let mut writer = csv::Writer::from_writer(Vec::new());
        writer.serialize(row)?;
        let bytes = writer
            .into_inner()
            .map_err(csv::IntoInnerError::into_error)?;
        let output = String::from_utf8(bytes)?;
        anyhow::ensure!(
            output == "client,available,held,total,locked\n1,1.5,0.0,1.5,false\n",
            "unexpected output: {output:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    fn account_balance_rejects_overflow() -> anyhow::Result<()> {
        let max = Decimal::MAX;
        let result = AccountBalance::try_from(Parts {
            client: ClientId::from(1),
            available: Balance::from(max),
            held: Balance::from(max),
            locked: false,
        });
        match result {
            Ok(row) => anyhow::bail!("expected OverflowError, got {row:?}"),
            Err(actual) => {
                let expected = OverflowError::Overflow {
                    available: max,
                    held: max,
                };
                anyhow::ensure!(actual == expected, "expected {expected:?}, got {actual:?}");
            }
        }
        Ok(())
    }
}
