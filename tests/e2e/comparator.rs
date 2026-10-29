//! Account CSV parsing and whole-vector comparison for the e2e harness.

use anyhow::Context as _;
use rust_decimal::Decimal;

/// One client account state row as emitted by the binary's stdout.
///
/// Amounts are [`Decimal`] so equality is value-based: `1.5` equals
/// `1.5000`. This remains a strict full-row check that is robust to any
/// trailing-zero padding in the output.
#[derive(Debug, PartialEq)]
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

/// Assert whole-vector equality between the expected and actual rows.
///
/// # Errors
///
/// Returns an error describing the full expected and actual row sets when
/// they differ.
pub(crate) fn compare_whole(
    case_name: &str,
    expected: &[AccountRow],
    actual: &[AccountRow],
) -> anyhow::Result<()> {
    anyhow::ensure!(
        actual == expected,
        "case `{case_name}`: account rows differ\n  expected: {expected:?}\n  actual:   {actual:?}"
    );
    Ok(())
}
