//! Service contract tests: the engine stub and balance-stream cancellation.

use rudzio::common::context::Suite;
use rudzio::runtime::futures::ThreadPool;

#[rudzio::suite([(runtime = ThreadPool::new, suite = Suite, test = rudzio::common::context::Test)])]
mod tests {
    use service::{
        BalanceStream as _, Engine, EngineError, RealBalances, RealBalancesError, ServeError,
        Service as _, TransactionBatch, TransactionStream,
    };
    use tokio_util::sync::CancellationToken;

    /// An always-empty transaction source for feeding the engine stub.
    struct EmptyTransactions;

    impl TransactionStream for EmptyTransactions {
        type Error = core::convert::Infallible;

        async fn next_batch(
            &mut self,
            _cancel: &CancellationToken,
        ) -> Result<Option<TransactionBatch<'_>>, Self::Error> {
            Ok(None)
        }
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
}
