//! Account CSV parsing and order-insensitive comparison for the e2e harness.

use anyhow::Context as _;
use rust_decimal::Decimal;
use std::cmp::Ordering;

/// One client account state row as emitted by the binary's stdout.
///
/// Amounts are [`Decimal`] so equality is value-based: `1.5` equals
/// `1.5000`. This remains a strict full-row check that is robust to any
/// trailing-zero padding in the output.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct AccountRow {
    /// Available funds.
    pub(crate) available: Decimal,
    /// Client id.
    pub(crate) client: u16,
    /// Held (disputed) funds.
    pub(crate) held: Decimal,
    /// Whether the account is locked after a chargeback.
    pub(crate) locked: bool,
    /// Total funds.
    pub(crate) total: Decimal,
}

/// Parse an account CSV body (header plus rows) into typed rows.
///
/// # Errors
///
/// Returns an error when the header is absent or mismatched, or when any
/// cell fails to parse as its declared type.
pub(crate) fn parse_accounts(data: &str) -> anyhow::Result<Vec<AccountRow>> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(data.as_bytes());
    let headers = reader.headers().context("account CSV has no header row")?;
    let expected_headers = ["client", "available", "held", "total", "locked"];
    let actual_headers: Vec<&str> = headers.iter().collect();
    anyhow::ensure!(
        actual_headers == expected_headers,
        "account CSV header mismatch: expected {expected_headers:?}, got {actual_headers:?}"
    );

    let mut rows = Vec::new();
    for record in reader.records() {
        let row = record.context("malformed account CSV row")?;
        rows.push(parse_row(&row)?);
    }
    Ok(rows)
}

/// Parse a single record into an [`AccountRow`].
fn parse_row(record: &csv::StringRecord) -> anyhow::Result<AccountRow> {
    Ok(AccountRow {
        available: parse_cell(record, 1, "available")?,
        client: parse_cell(record, 0, "client")?,
        held: parse_cell(record, 2, "held")?,
        locked: parse_cell(record, 4, "locked")?,
        total: parse_cell(record, 3, "total")?,
    })
}

/// Parse the cell at `index` as `T`, carrying the column name in the error.
fn parse_cell<T>(record: &csv::StringRecord, index: usize, column: &str) -> anyhow::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let cell = record
        .get(index)
        .with_context(|| format!("account CSV row is missing the `{column}` column"))?;
    match cell.parse::<T>() {
        Ok(value) => Ok(value),
        Err(err) => Err(anyhow::anyhow!(
            "account CSV cell `{column}` = `{cell}` is invalid: {err}"
        )),
    }
}

/// Assert that `expected` and `actual` hold the same account rows, regardless
/// of order.
///
/// Row ordering is unspecified, so the comparison is a multiset check: each
/// row must appear with the same multiplicity on both sides. Equality stays
/// value-based for [`Decimal`] fields, so `1.5` matches `1.5000`.
///
/// # Errors
///
/// Returns an error naming the rows missing from `actual` and the rows
/// unexpectedly present in `actual` when the multisets differ.
pub(crate) fn compare_whole(
    case_name: &str,
    expected: &[AccountRow],
    actual: &[AccountRow],
) -> anyhow::Result<()> {
    let mut expected_sorted: Vec<&AccountRow> = expected.iter().collect();
    let mut actual_sorted: Vec<&AccountRow> = actual.iter().collect();
    expected_sorted.sort_unstable();
    actual_sorted.sort_unstable();

    let mut missing: Vec<&AccountRow> = Vec::new();
    let mut unexpected: Vec<&AccountRow> = Vec::new();
    let mut expected_iter = expected_sorted.iter().copied();
    let mut actual_iter = actual_sorted.iter().copied();
    let mut expected_item = expected_iter.next();
    let mut actual_item = actual_iter.next();
    loop {
        match (expected_item, actual_item) {
            (Some(expected_row), Some(actual_row)) => match expected_row.cmp(actual_row) {
                Ordering::Equal => {
                    expected_item = expected_iter.next();
                    actual_item = actual_iter.next();
                }
                Ordering::Less => {
                    missing.push(expected_row);
                    expected_item = expected_iter.next();
                }
                Ordering::Greater => {
                    unexpected.push(actual_row);
                    actual_item = actual_iter.next();
                }
            },
            (Some(expected_row), None) => {
                missing.push(expected_row);
                expected_item = expected_iter.next();
            }
            (None, Some(actual_row)) => {
                unexpected.push(actual_row);
                actual_item = actual_iter.next();
            }
            (None, None) => break,
        }
    }

    anyhow::ensure!(
        missing.is_empty() && unexpected.is_empty(),
        "case `{case_name}`: account rows differ as multisets\n  missing from actual: {missing:?}\n  unexpected in actual: {unexpected:?}"
    );
    Ok(())
}

#[rudzio::suite([(runtime = rudzio::runtime::futures::ThreadPool::new, suite = rudzio::common::context::Suite, test = rudzio::common::context::Test)])]
mod tests {
    use super::{AccountRow, compare_whole};
    use rust_decimal::Decimal;

    /// Build a client row whose available and total equal `amount`; held is zero.
    fn row(client: u16, amount: i64) -> AccountRow {
        AccountRow {
            available: Decimal::from(amount),
            client,
            held: Decimal::ZERO,
            total: Decimal::from(amount),
            locked: false,
        }
    }

    /// Run the comparison and return the error message, failing if it passed.
    fn mismatch_message(
        case_name: &str,
        expected: &[AccountRow],
        actual: &[AccountRow],
    ) -> anyhow::Result<String> {
        match compare_whole(case_name, expected, actual) {
            Ok(()) => anyhow::bail!("expected account rows to differ, but comparison passed"),
            Err(err) => Ok(format!("{err}")),
        }
    }

    #[rudzio::test]
    async fn same_rows_in_different_order_pass(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let expected = vec![row(1, 10), row(2, 20)];
        let actual = vec![row(2, 20), row(1, 10)];
        compare_whole("case", &expected, &actual)
    }

    #[rudzio::test]
    async fn missing_row_fails_naming_the_row(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let missing = row(2, 20);
        let missing_debug = format!("{missing:?}");
        let expected = vec![row(1, 10), missing];
        let actual = vec![row(1, 10)];
        let message = mismatch_message("case", &expected, &actual)?;
        anyhow::ensure!(
            message.contains("missing from actual"),
            "error did not report missing rows: {message}"
        );
        anyhow::ensure!(
            message.contains(&missing_debug),
            "error did not name the missing row {missing_debug}: {message}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn extra_row_fails_naming_the_row(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let extra = row(2, 20);
        let extra_debug = format!("{extra:?}");
        let expected = vec![row(1, 10)];
        let actual = vec![row(1, 10), extra];
        let message = mismatch_message("case", &expected, &actual)?;
        anyhow::ensure!(
            message.contains("unexpected in actual"),
            "error did not report unexpected rows: {message}"
        );
        anyhow::ensure!(
            message.contains(&extra_debug),
            "error did not name the unexpected row {extra_debug}: {message}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn duplicate_row_fails_counting_multiplicity(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let duplicated = row(1, 10);
        let duplicated_debug = format!("{duplicated:?}");
        let expected = vec![row(1, 10)];
        let actual = vec![row(1, 10), duplicated];
        let message = mismatch_message("case", &expected, &actual)?;
        anyhow::ensure!(
            message.contains("unexpected in actual"),
            "error did not report the duplicated row: {message}"
        );
        anyhow::ensure!(
            message.contains(&duplicated_debug),
            "error did not name the duplicated row {duplicated_debug}: {message}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn decimal_scale_mismatch_passes(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let expected = vec![AccountRow {
            available: Decimal::new(15, 1),
            client: 1,
            held: Decimal::ZERO,
            total: Decimal::new(15, 1),
            locked: false,
        }];
        let actual = vec![AccountRow {
            available: Decimal::new(15000, 4),
            client: 1,
            held: Decimal::ZERO,
            total: Decimal::new(15000, 4),
            locked: false,
        }];
        compare_whole("case", &expected, &actual)
    }
}
