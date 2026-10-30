//! Service contract tests: delegation, dispatch, and cancellation.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use record_processor_domain::client_id::ClientId;
    use record_processor_domain::input::Transaction;
    use record_processor_domain::output::{AccountBalance, Balance, Parts};
    use rust_decimal::Decimal;
    use service::{
        BalanceStream as _, Balances, BalancesError, Engine, EngineError, Mock, MockBalances,
        MockBalancesError, MockServiceError, RealBalances, RealBalancesError, ServeDispatchError,
        ServeError, Service as _, ServiceType, TransactionStream,
    };
    use std::str::FromStr as _;
    use tokio_util::sync::CancellationToken;

    /// An always-empty transaction source for feeding the engine stub.
    struct EmptyTransactions;

    impl TransactionStream for EmptyTransactions {
        type Error = core::convert::Infallible;

        async fn next(
            &mut self,
            _cancel: &CancellationToken,
        ) -> Result<Option<Transaction>, Self::Error> {
            Ok(None)
        }
    }

    /// Build a single account balance row for the mock stream.
    fn balance() -> anyhow::Result<AccountBalance> {
        Ok(AccountBalance::try_from(Parts {
            client: ClientId::from(1),
            available: Balance::from(Decimal::from_str("1.5")?),
            held: Balance::from(Decimal::from_str("0.0")?),
            locked: false,
        })?)
    }

    #[rudzio::test]
    async fn engine_serve_is_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let result = Engine.serve(EmptyTransactions, &token).await;
        anyhow::ensure!(
            result == Err(ServeError::Impl(EngineError::Unimplemented)),
            "expected Unimplemented, got {result:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn service_type_real_dispatches_to_engine(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let result = ServiceType::Real(Engine)
            .serve(EmptyTransactions, &token)
            .await;
        anyhow::ensure!(
            result
                == Err(ServeDispatchError::Real(ServeError::Impl(
                    EngineError::Unimplemented
                ))),
            "expected Unimplemented, got {result:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn service_type_mock_dispatches_to_mock(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let result = ServiceType::Mock(Mock)
            .serve(EmptyTransactions, &token)
            .await;
        anyhow::ensure!(
            result == Err(ServeDispatchError::Mock(MockServiceError::Unimplemented)),
            "expected Unimplemented, got {result:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn real_balances_yield_unimplemented(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut balances = RealBalances;
        let item = balances.next(&token).await;
        anyhow::ensure!(
            item == Err(RealBalancesError::Unimplemented),
            "expected Unimplemented, got {item:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn real_balances_yield_cancelled(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        token.cancel();
        let mut balances = RealBalances;
        let item = balances.next(&token).await;
        anyhow::ensure!(
            item == Err(RealBalancesError::Cancelled),
            "expected Cancelled, got {item:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn mock_balances_yield_item_then_none(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let row = balance()?;
        let mut balances = MockBalances::from(Some(row));
        let first = balances.next(&token).await;
        anyhow::ensure!(
            first == Ok(Some(row)),
            "expected one balance, got {first:?}"
        );
        let second = balances.next(&token).await;
        anyhow::ensure!(second == Ok(None), "expected end, got {second:?}");
        Ok(())
    }

    #[rudzio::test]
    async fn mock_balances_yield_cancelled(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        token.cancel();
        let mut balances = MockBalances::from(None);
        let item = balances.next(&token).await;
        anyhow::ensure!(
            item == Err(MockBalancesError::Cancelled),
            "expected Cancelled, got {item:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn balances_enum_dispatches_real(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut balances = Balances::Real(RealBalances);
        let item = balances.next(&token).await;
        anyhow::ensure!(
            item == Err(BalancesError::Real(RealBalancesError::Unimplemented)),
            "expected Unimplemented, got {item:?}"
        );
        Ok(())
    }

    #[rudzio::test]
    async fn balances_enum_dispatches_mock(
        _ctx: &rudzio::common::context::Test,
    ) -> anyhow::Result<()> {
        let token = CancellationToken::new();
        let mut balances = Balances::Mock(MockBalances::from(None));
        let item = balances.next(&token).await;
        anyhow::ensure!(item == Ok(None), "expected end, got {item:?}");
        Ok(())
    }
}
