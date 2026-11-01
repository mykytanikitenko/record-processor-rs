//! End-to-end test suite: one rudzio test per fixture case.
//!
//! Each test drives the compiled binary over a fixture's `input.csv` and
//! checks the result against the case's declared `outcome` in `case.toml`.

use anyhow::Context as _;
use std::path::Path;

use crate::case_loader::{LoadedCase, Outcome};

/// Run a fixture case to completion according to its declared outcome.
async fn run_case(name: &str) -> anyhow::Result<()> {
    let bin = crate::runner::resolve_bin();
    let case = crate::case_loader::load_case(name).await?;
    match &case.meta.outcome {
        Outcome::Success => run_success(&bin, name, &case).await,
        Outcome::Error => run_error(&bin, name, &case).await,
        Outcome::PendingDecision => run_pending(&case).await,
    }
}

/// Run the binary and compare its stdout against the expected account rows.
async fn run_success(bin: &Path, name: &str, case: &LoadedCase) -> anyhow::Result<()> {
    let expected_path = case
        .expected
        .as_ref()
        .context("success case has no expected.csv")?;
    let expected_bytes = compio::fs::read(expected_path)
        .await
        .with_context(|| format!("case `{name}`: failed to read expected.csv"))?;
    let expected_data = String::from_utf8(expected_bytes)
        .with_context(|| format!("case `{name}`: expected.csv is not UTF-8"))?;
    let expected = crate::comparator::parse_accounts(&expected_data)?;

    let output = crate::runner::run_binary(bin, &case.input).await?;
    anyhow::ensure!(
        output.status_success,
        "case `{name}`: expected a successful exit, got status {:?}",
        output.status_code
    );
    anyhow::ensure!(
        output.stderr.is_empty(),
        "case `{name}`: expected empty stderr, got:\n{}",
        output.stderr
    );

    let actual = crate::comparator::parse_accounts(&output.stdout)?;
    crate::comparator::compare_whole(name, &expected, &actual)
}

/// Run the binary and require a non-zero exit with empty stdout.
async fn run_error(bin: &Path, name: &str, case: &LoadedCase) -> anyhow::Result<()> {
    let output = crate::runner::run_binary(bin, &case.input).await?;
    anyhow::ensure!(
        !output.status_success,
        "case `{name}`: expected a non-zero exit, got status {:?}",
        output.status_code
    );
    anyhow::ensure!(
        output.stdout.is_empty(),
        "case `{name}`: expected empty stdout, got:\n{}",
        output.stdout
    );
    Ok(())
}

/// Fail loudly: the spec leaves this case's outcome undecided.
async fn run_pending(case: &LoadedCase) -> anyhow::Result<()> {
    let decision = case
        .meta
        .decision
        .as_deref()
        .context("pending-decision case has no decision id")?;
    let question = case
        .meta
        .question
        .as_deref()
        .context("pending-decision case has no question")?;
    anyhow::bail!("pending decision {decision}: {question}")
}

#[rudzio::suite([(runtime = rudzio::runtime::compio::Runtime::new, suite = rudzio::common::context::Suite, test = rudzio::common::context::Test)])]
mod tests {
    use super::run_case;

    // Spec example, verbatim.

    #[rudzio::test]
    async fn spec_example(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("001-spec-example").await
    }

    // Deposits.

    #[rudzio::test]
    async fn deposit_single(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("010-deposit-single").await
    }

    #[rudzio::test]
    async fn deposit_multiple_same_client(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        run_case("011-deposit-multiple-same-client").await
    }

    #[rudzio::test]
    async fn deposit_multiple_clients(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("012-deposit-multiple-clients").await
    }

    #[rudzio::test]
    async fn deposit_interleaved_unordered(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        run_case("013-deposit-interleaved-unordered").await
    }

    #[rudzio::test]
    async fn deposit_max_4dp(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("014-deposit-max-4dp").await
    }

    #[rudzio::test]
    async fn deposit_whitespace(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("015-deposit-whitespace").await
    }

    #[rudzio::test]
    async fn deposit_trailing_zero_precision(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        run_case("016-deposit-trailing-zero-precision").await
    }

    #[rudzio::test]
    async fn deposit_large_values(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("017-deposit-large-values").await
    }

    #[rudzio::test]
    async fn deposit_u16_max_client(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("018-deposit-u16-max-client").await
    }

    #[rudzio::test]
    async fn deposit_u32_max_tx(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("019-deposit-u32-max-tx").await
    }

    #[rudzio::test]
    async fn deposit_client_zero(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("020-deposit-client-zero").await
    }

    #[rudzio::test]
    async fn deposit_tx_zero(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("021-deposit-tx-zero").await
    }

    // Withdrawals.

    #[rudzio::test]
    async fn withdrawal_sufficient(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("030-withdrawal-sufficient").await
    }

    #[rudzio::test]
    async fn withdrawal_exact_to_zero(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("031-withdrawal-exact-to-zero").await
    }

    #[rudzio::test]
    async fn withdrawal_insufficient(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("032-withdrawal-insufficient").await
    }

    #[rudzio::test]
    async fn withdrawal_never_seen_client(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        run_case("033-withdrawal-never-seen-client").await
    }

    // Disputes, resolutions, and chargebacks.

    #[rudzio::test]
    async fn dispute_on_deposit(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("040-dispute-on-deposit").await
    }

    #[rudzio::test]
    async fn dispute_unknown_tx(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("041-dispute-unknown-tx").await
    }

    #[rudzio::test]
    async fn resolve_after_dispute(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("042-resolve-after-dispute").await
    }

    #[rudzio::test]
    async fn chargeback_after_dispute(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("043-chargeback-after-dispute").await
    }

    #[rudzio::test]
    async fn resolve_without_dispute(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("044-resolve-without-dispute").await
    }

    #[rudzio::test]
    async fn chargeback_without_dispute(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        run_case("045-chargeback-without-dispute").await
    }

    #[rudzio::test]
    async fn resolve_chargeback_unknown_tx(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        run_case("046-resolve-chargeback-unknown-tx").await
    }

    #[rudzio::test]
    async fn multiple_concurrent_disputes(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        run_case("047-multiple-concurrent-disputes").await
    }

    // Mixed scenario.

    #[rudzio::test]
    async fn mixed_multi_client(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("050-mixed-multi-client").await
    }

    // Header-only.

    #[rudzio::test]
    async fn header_only(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("060-header-only").await
    }

    // Malformed input — abort the run with a non-zero exit and empty stdout.

    #[rudzio::test]
    async fn malformed_unknown_type(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("061-malformed-unknown-type").await
    }

    #[rudzio::test]
    async fn malformed_literal_eof(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("062-malformed-literal-eof").await
    }

    #[rudzio::test]
    async fn malformed_amount_on_dispute(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        run_case("063-malformed-amount-on-dispute").await
    }

    #[rudzio::test]
    async fn malformed_missing_amount_deposit(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        run_case("064-malformed-missing-amount-deposit").await
    }

    #[rudzio::test]
    async fn malformed_negative_amount(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("065-malformed-negative-amount").await
    }

    #[rudzio::test]
    async fn malformed_zero_amount(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("066-malformed-zero-amount").await
    }

    #[rudzio::test]
    async fn malformed_over_4dp(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("067-malformed-over-4dp").await
    }

    #[rudzio::test]
    async fn malformed_non_numeric_client(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        run_case("068-malformed-non-numeric-client").await
    }

    #[rudzio::test]
    async fn malformed_non_numeric_tx(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("069-malformed-non-numeric-tx").await
    }

    #[rudzio::test]
    async fn malformed_client_over_u16(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("070-malformed-client-over-u16").await
    }

    #[rudzio::test]
    async fn malformed_tx_over_u32(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("071-malformed-tx-over-u32").await
    }

    // Ambiguous — the spec leaves the outcome open.

    #[rudzio::test]
    async fn dispute_withdrawal(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("080-dispute-withdrawal").await
    }

    #[rudzio::test]
    async fn dispute_insufficient_available(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        run_case("081-dispute-insufficient-available").await
    }

    #[rudzio::test]
    async fn locked_account_operations(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("082-locked-account-operations").await
    }

    #[rudzio::test]
    async fn mismatched_client_reference(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        run_case("083-mismatched-client-reference").await
    }

    #[rudzio::test]
    async fn duplicate_tx_id(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("084-duplicate-tx-id").await
    }

    #[rudzio::test]
    async fn redispute_after_resolve(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("085-redispute-after-resolve").await
    }

    #[rudzio::test]
    async fn dispute_after_chargeback(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("086-dispute-after-chargeback").await
    }

    #[rudzio::test]
    async fn double_dispute(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("087-double-dispute").await
    }

    #[rudzio::test]
    async fn dispute_failed_withdrawal(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("088-dispute-failed-withdrawal").await
    }

    #[rudzio::test]
    async fn ignored_row_client_output(_ctx: &rudzio::common::context::Test) -> anyhow::Result<()> {
        run_case("089-ignored-row-client-output").await
    }
}
