//! File-backed transaction reader tests: the file is streamed through the
//! reader's single reused buffer, never read whole.

use std::convert::Infallible;
use std::path::{Path, PathBuf};

use rudzio::Config;
use rudzio::context;
use rudzio::runtime::Runtime;
use rudzio::tokio_util::sync::CancellationToken;
use rudzio::tokio_util::task::TaskTracker;

/// 22-byte header + 16-byte rows: the third row straddles the first 64-byte
/// refill (bytes 54..70).
const CONTENT: &str = "type,client,tx,amount\n\
    deposit,1,1,1.0\n\
    deposit,2,2,2.5\n\
    deposit,3,3,3.5\n\
    deposit,4,4,4.5\n\
    deposit,5,5,5.5\n\
    deposit,6,6,6.5\n\
    deposit,7,7,7.5\n\
    deposit,8,8,8.5\n";

/// Suite context: a temp input file written once in setup and removed in
/// teardown.
struct FileSuite<'suite_context, R>
where
    R: Runtime<'suite_context> + Sync,
{
    /// Root cancellation token whose children are handed out to per-test contexts.
    cancel: CancellationToken,
    /// Path of the temp input file, shared by every test.
    path: PathBuf,
    /// Borrow of the async runtime driving the suite context.
    rt: &'suite_context R,
    /// Tracks spawned background tasks so teardown can wait for them.
    tracker: TaskTracker,
}

/// Per-test context: borrows the suite's temp-file path.
struct FileTest<'test_context, R>
where
    R: Runtime<'test_context> + Sync,
{
    /// Child cancellation token scoped to this test.
    cancel: CancellationToken,
    /// Resolved CLI/env configuration, handed down from the suite.
    config: &'test_context Config,
    /// The suite's temp-file path, borrowed for this test.
    path: &'test_context Path,
    /// Borrow of the async runtime driving the test.
    rt: &'test_context R,
    /// Suite-shared task tracker, borrowed rather than cloned.
    tracker: &'test_context TaskTracker,
}

impl<'suite_context, R> std::fmt::Debug for FileSuite<'suite_context, R>
where
    R: Runtime<'suite_context> + Sync,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSuite").finish_non_exhaustive()
    }
}

impl<'test_context, R> std::fmt::Debug for FileTest<'test_context, R>
where
    R: Runtime<'test_context> + Sync,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileTest").finish_non_exhaustive()
    }
}

impl<'suite_context, R> context::Suite<'suite_context, R> for FileSuite<'suite_context, R>
where
    R: for<'rt> Runtime<'rt> + Sync,
{
    type ContextError = Infallible;
    type SetupError = anyhow::Error;
    type TeardownError = anyhow::Error;
    type Test<'test_context>
        = FileTest<'test_context, R>
    where
        Self: 'test_context;

    #[inline]
    fn cancel_token(&self) -> &CancellationToken {
        &self.cancel
    }

    #[inline]
    async fn context<'test_context>(
        &'test_context self,
        cancel: CancellationToken,
        config: &'test_context Config,
    ) -> Result<FileTest<'test_context, R>, Infallible> {
        Ok(FileTest {
            cancel,
            config,
            path: self.path.as_path(),
            rt: self.rt,
            tracker: &self.tracker,
        })
    }

    #[inline]
    fn rt(&self) -> &'suite_context R {
        self.rt
    }

    #[inline]
    async fn setup(
        rt: &'suite_context R,
        cancel: CancellationToken,
        _config: &'suite_context Config,
    ) -> Result<Self, anyhow::Error> {
        let path =
            std::env::temp_dir().join(format!("record-processor-{}.csv", std::process::id()));
        rt.block_on(compio::fs::write(&path, CONTENT)).0?;
        Ok(Self {
            cancel: cancel.child_token(),
            path,
            rt,
            tracker: TaskTracker::new(),
        })
    }

    #[inline]
    async fn teardown(self, _cancel: CancellationToken) -> Result<(), anyhow::Error> {
        self.cancel.cancel();
        let _closed: bool = self.tracker.close();
        self.tracker.wait().await;
        self.rt.block_on(compio::fs::remove_file(&self.path))?;
        Ok(())
    }

    #[inline]
    fn tracker(&self) -> &TaskTracker {
        &self.tracker
    }
}

impl<'test_context, R> context::Test<'test_context, R> for FileTest<'test_context, R>
where
    R: Runtime<'test_context> + Sync,
{
    type TeardownError = Infallible;

    #[inline]
    fn cancel_token(&self) -> &CancellationToken {
        &self.cancel
    }

    #[inline]
    fn config(&self) -> &Config {
        self.config
    }

    #[inline]
    fn rt(&self) -> &'test_context R {
        self.rt
    }

    #[inline]
    async fn teardown(self, _cancel: CancellationToken) -> Result<(), Infallible> {
        self.cancel.cancel();
        Ok(())
    }

    #[inline]
    fn tracker(&self) -> &TaskTracker {
        self.tracker
    }
}

#[rudzio::suite([(runtime = rudzio::runtime::compio::Runtime::new, suite = FileSuite, test = FileTest)])]
mod tests {
    use compio::io::AsyncRead;
    use rudzio::context::Test as _;
    use rust_decimal::Decimal;
    use service::{Amount, ClientId, Transaction, TxId};
    use std::str::FromStr as _;
    use tokio_util::sync::CancellationToken;
    use transport::{Input, Reader};

    use super::FileTest;

    /// Build a deposit transaction for an expected-value assertion.
    fn deposit(client: u16, tx: u32, amount: &str) -> anyhow::Result<Transaction> {
        Ok(Transaction::Deposit {
            client: ClientId::from(client),
            tx: TxId::from(tx),
            amount: Amount::try_from(Decimal::from_str(amount)?)?,
        })
    }

    /// Drive a reader to completion, dropping the end-of-input marker.
    async fn read_all<S>(mut reader: Reader<S>) -> anyhow::Result<Vec<Transaction>>
    where
        S: AsyncRead,
    {
        let token = CancellationToken::new();
        let mut transactions = Vec::new();
        loop {
            match reader.next(&token).await? {
                Some(Transaction::Eof) => {}
                Some(transaction) => transactions.push(transaction),
                None => return Ok(transactions),
            }
        }
    }

    #[rudzio::test]
    async fn file_larger_than_buffer_streams_across_refills(
        ctx: &FileTest,
    ) -> anyhow::Result<()> {
        let file = ctx.block_on(compio::fs::File::open(ctx.path))?;
        let transactions =
            ctx.block_on(read_all(Reader::<compio::fs::Stdin>::from(Input::from(file))))?;
        anyhow::ensure!(
            transactions
                == [
                    deposit(1, 1, "1.0")?,
                    deposit(2, 2, "2.5")?,
                    deposit(3, 3, "3.5")?,
                    deposit(4, 4, "4.5")?,
                    deposit(5, 5, "5.5")?,
                    deposit(6, 6, "6.5")?,
                    deposit(7, 7, "7.5")?,
                    deposit(8, 8, "8.5")?,
                ],
            "unexpected transactions: {transactions:?}"
        );
        Ok(())
    }
}
