//! Input-side domain tests: amount validation and CSV deserialization.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use record_processor_domain::client_id::ClientId;
    use record_processor_domain::input::{Amount, ConstructionError, Transaction, TxId};
    use rust_decimal::Decimal;
    use std::str::FromStr as _;

    /// Parses CSV text into a sequence of transactions.
    fn parse(csv_text: &str) -> anyhow::Result<Vec<Transaction>> {
        let mut reader = csv::ReaderBuilder::new()
            .trim(csv::Trim::All)
            .flexible(true)
            .from_reader(csv_text.as_bytes());
        let mut transactions = Vec::new();
        for record in reader.deserialize::<Transaction>() {
            transactions.push(record?);
        }
        Ok(transactions)
    }

    /// Asserts that parsing fails with an error message containing `fragment`.
    fn expect_error(csv_text: &str, fragment: &str) -> anyhow::Result<()> {
        match parse(csv_text) {
            Ok(transactions) => {
                anyhow::bail!("expected deserialization error, got {transactions:?}")
            }
            Err(error) => {
                let message = format!("{error}");
                anyhow::ensure!(
                    message.contains(fragment),
                    "expected error containing {fragment:?}, got {message:?}"
                );
            }
        }
        Ok(())
    }

    /// Asserts that `result` equals `expected`.
    fn assert_amount_error(
        result: Result<Amount, ConstructionError>,
        expected: ConstructionError,
    ) -> anyhow::Result<()> {
        match result {
            Ok(amount) => anyhow::bail!("expected ConstructionError, got {amount:?}"),
            Err(actual) => {
                anyhow::ensure!(actual == expected, "expected {expected:?}, got {actual:?}");
            }
        }
        Ok(())
    }

    #[rudzio::test]
    fn accessors_expose_raw_values() -> anyhow::Result<()> {
        anyhow::ensure!(*ClientId::from(7) == 7, "client id get mismatch");
        anyhow::ensure!(*TxId::from(11) == 11, "tx id get mismatch");
        Ok(())
    }

    #[rudzio::test]
    fn amount_rejects_too_many_decimal_places() -> anyhow::Result<()> {
        let result = Amount::try_from(Decimal::from_str("1.00005")?);
        assert_amount_error(
            result,
            ConstructionError::TooManyDecimalPlaces {
                value: Decimal::from_str("1.00005")?,
            },
        )
    }

    #[rudzio::test]
    fn amount_rejects_zero() -> anyhow::Result<()> {
        let result = Amount::try_from(Decimal::from_str("0")?);
        assert_amount_error(
            result,
            ConstructionError::NonPositive {
                value: Decimal::from_str("0")?,
            },
        )
    }

    #[rudzio::test]
    fn amount_rejects_negative() -> anyhow::Result<()> {
        let result = Amount::try_from(Decimal::from_str("-1.5")?);
        assert_amount_error(
            result,
            ConstructionError::NonPositive {
                value: Decimal::from_str("-1.5")?,
            },
        )
    }

    #[rudzio::test]
    fn amount_accepts_four_decimal_places() -> anyhow::Result<()> {
        let amount = Amount::try_from(Decimal::from_str("1.2345")?)?;
        anyhow::ensure!(
            *amount == Decimal::from_str("1.2345")?,
            "four-dp amount mismatch"
        );
        Ok(())
    }

    #[rudzio::test]
    fn amount_normalizes_trailing_zeros() -> anyhow::Result<()> {
        let amount = Amount::try_from(Decimal::from_str("1.50000")?)?;
        anyhow::ensure!(
            *amount == Decimal::from_str("1.5")?,
            "normalized amount mismatch"
        );
        Ok(())
    }

    #[rudzio::test]
    fn deserializes_deposit() -> anyhow::Result<()> {
        let transactions = parse("type, client, tx, amount\ndeposit, 1, 1, 1.0\n")?;
        let expected = vec![Transaction::Deposit {
            client: ClientId::from(1),
            tx: TxId::from(1),
            amount: Amount::try_from(Decimal::from_str("1.0")?)?,
        }];
        anyhow::ensure!(
            transactions == expected,
            "expected {expected:?}, got {transactions:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    fn deserializes_withdrawal() -> anyhow::Result<()> {
        let transactions = parse("type, client, tx, amount\nwithdrawal, 2, 2, 2.0\n")?;
        let expected = vec![Transaction::Withdrawal {
            client: ClientId::from(2),
            tx: TxId::from(2),
            amount: Amount::try_from(Decimal::from_str("2.0")?)?,
        }];
        anyhow::ensure!(
            transactions == expected,
            "expected {expected:?}, got {transactions:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    fn deserializes_dispute() -> anyhow::Result<()> {
        let transactions = parse("type, client, tx, amount\ndispute, 1, 1\n")?;
        let expected = vec![Transaction::Dispute {
            client: ClientId::from(1),
            tx: TxId::from(1),
        }];
        anyhow::ensure!(
            transactions == expected,
            "expected {expected:?}, got {transactions:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    fn deserializes_resolve() -> anyhow::Result<()> {
        let transactions = parse("type, client, tx, amount\nresolve, 1, 1\n")?;
        let expected = vec![Transaction::Resolve {
            client: ClientId::from(1),
            tx: TxId::from(1),
        }];
        anyhow::ensure!(
            transactions == expected,
            "expected {expected:?}, got {transactions:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    fn deserializes_chargeback() -> anyhow::Result<()> {
        let transactions = parse("type, client, tx, amount\nchargeback, 1, 1\n")?;
        let expected = vec![Transaction::Chargeback {
            client: ClientId::from(1),
            tx: TxId::from(1),
        }];
        anyhow::ensure!(
            transactions == expected,
            "expected {expected:?}, got {transactions:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    fn deserializes_multiple_rows_in_order() -> anyhow::Result<()> {
        let transactions = parse(
            "type, client, tx, amount\n\
             deposit, 1, 1, 1.0\n\
             withdrawal, 1, 4, 1.5\n\
             dispute, 1, 1\n",
        )?;
        let expected = vec![
            Transaction::Deposit {
                client: ClientId::from(1),
                tx: TxId::from(1),
                amount: Amount::try_from(Decimal::from_str("1.0")?)?,
            },
            Transaction::Withdrawal {
                client: ClientId::from(1),
                tx: TxId::from(4),
                amount: Amount::try_from(Decimal::from_str("1.5")?)?,
            },
            Transaction::Dispute {
                client: ClientId::from(1),
                tx: TxId::from(1),
            },
        ];
        anyhow::ensure!(
            transactions == expected,
            "expected {expected:?}, got {transactions:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    fn accepts_whitespace_around_fields() -> anyhow::Result<()> {
        let transactions = parse("type, client, tx, amount\n deposit, 1 , 1 , 1.5 \n")?;
        let expected = vec![Transaction::Deposit {
            client: ClientId::from(1),
            tx: TxId::from(1),
            amount: Amount::try_from(Decimal::from_str("1.5")?)?,
        }];
        anyhow::ensure!(
            transactions == expected,
            "expected {expected:?}, got {transactions:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    fn accepts_four_dp_boundary_via_csv() -> anyhow::Result<()> {
        let transactions = parse("type, client, tx, amount\ndeposit, 1, 1, 1.2345\n")?;
        let expected = vec![Transaction::Deposit {
            client: ClientId::from(1),
            tx: TxId::from(1),
            amount: Amount::try_from(Decimal::from_str("1.2345")?)?,
        }];
        anyhow::ensure!(
            transactions == expected,
            "expected {expected:?}, got {transactions:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    fn rejects_eof_row() -> anyhow::Result<()> {
        expect_error(
            "type, client, tx, amount\neof, 1, 1\n",
            "unknown transaction type: eof",
        )
    }

    #[rudzio::test]
    fn rejects_unknown_type() -> anyhow::Result<()> {
        expect_error(
            "type, client, tx, amount\nfoo, 1, 1, 1.0\n",
            "unknown transaction type: foo",
        )
    }

    #[rudzio::test]
    fn rejects_deposit_without_amount() -> anyhow::Result<()> {
        expect_error(
            "type, client, tx, amount\ndeposit, 1, 1\n",
            "missing an amount",
        )
    }

    #[rudzio::test]
    fn rejects_withdrawal_without_amount() -> anyhow::Result<()> {
        expect_error(
            "type, client, tx, amount\nwithdrawal, 1, 1\n",
            "missing an amount",
        )
    }

    #[rudzio::test]
    fn rejects_dispute_with_amount() -> anyhow::Result<()> {
        expect_error(
            "type, client, tx, amount\ndispute, 1, 1, 1.0\n",
            "must not carry an amount",
        )
    }

    #[rudzio::test]
    fn rejects_resolve_with_amount() -> anyhow::Result<()> {
        expect_error(
            "type, client, tx, amount\nresolve, 1, 1, 1.0\n",
            "must not carry an amount",
        )
    }

    #[rudzio::test]
    fn rejects_chargeback_with_amount() -> anyhow::Result<()> {
        expect_error(
            "type, client, tx, amount\nchargeback, 1, 1, 1.0\n",
            "must not carry an amount",
        )
    }

    #[rudzio::test]
    fn rejects_client_out_of_range() -> anyhow::Result<()> {
        expect_error(
            "type, client, tx, amount\ndeposit, 70000, 1, 1.0\n",
            "too large to fit in target type",
        )
    }

    #[rudzio::test]
    fn rejects_tx_out_of_range() -> anyhow::Result<()> {
        expect_error(
            "type, client, tx, amount\ndeposit, 1, 5000000000, 1.0\n",
            "too large to fit in target type",
        )
    }

    #[rudzio::test]
    fn rejects_amount_with_too_many_places_via_csv() -> anyhow::Result<()> {
        expect_error(
            "type, client, tx, amount\ndeposit, 1, 1, 1.00005\n",
            "more than four decimal places",
        )
    }
}
