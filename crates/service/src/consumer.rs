//! The consumer contract and its production engine.
//!
//! A [`Consumer`] drains its transaction queue, applying each record to the
//! engine [`State`](crate::engine_state::State), and stops at the end-of-stream
//! marker. The resulting per-client balances are projected lazily through a
//! [`BalanceStream`], one row at a time.

use core::error::Error;

use record_processor_domain::client_id::ClientId;
use record_processor_domain::input::Transaction;
use record_processor_domain::output::{AccountBalance, Balance, OverflowError, Parts};
use rust_decimal::Decimal;
use storage::offset::Offset;
use storage::queue::Queue;
use tokio_util::sync::CancellationToken;

use crate::engine_state::{
    DepositState, DepositStateParts, DepositStatus, ServiceState, ServiceStateParts, State,
};
use crate::stream::{Balances, Transactions};

/// A payments consumer that drains its transaction stream into engine state
/// and projects the resulting per-client balances.
pub trait Consumer<T>
where
    T: Transactions,
{
    /// The lazily-projected balance stream produced by [`Consumer::consume`].
    type Balances: Balances;
    /// The error produced by [`Consumer::consume`].
    type ConsumeError: Error;

    /// Drain every transaction batch, applying each record to engine state,
    /// then return a lazy stream of the resulting per-client balances.
    async fn consume(
        self,
        cancel: &CancellationToken,
        transactions: &mut T,
    ) -> Result<Self::Balances, Self::ConsumeError>;
}

/// The account-balance rows a consumer projects, yielded one row at a time.
#[derive(Debug)]
pub struct BalanceStream<S> {
    /// The client most recently yielded, used as the cursor for the next row.
    next: Option<ClientId>,
    /// The single projected row handed to the caller on the previous batch.
    slot: Option<AccountBalance>,
    /// The engine state balances are projected from.
    state: S,
}

/// Failure to project a balance row from the engine state.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BalanceError<S>
where
    S: State,
{
    /// Enumerating the client records failed.
    #[error("enumerating the client records failed: {0}")]
    Clients(S::ClientsError),
    /// Projecting a client's balance total overflowed.
    #[error(transparent)]
    Overflow(#[from] OverflowError),
}

impl<S> Balances for BalanceStream<S>
where
    S: State + core::fmt::Debug,
{
    type Error = BalanceError<S>;

    #[inline]
    async fn next_batch<'stream>(
        &'stream mut self,
        cancel: &CancellationToken,
    ) -> Result<Option<&'stream [AccountBalance]>, Self::Error> {
        let Some((client, state)) = self
            .state
            .next_client(self.next, cancel)
            .await
            .map_err(BalanceError::Clients)?
        else {
            return Ok(None);
        };
        self.next = Some(client);
        let row = AccountBalance::try_from(Parts {
            client,
            available: Balance::from(state.available()),
            held: Balance::from(state.held()),
            locked: state.locked(),
        })?;
        self.slot = Some(row);
        Ok(self.slot.as_ref().map(core::slice::from_ref))
    }
}

/// The production consumer: a transaction queue drained into engine state.
#[derive(Debug)]
pub struct Engine<Q, S> {
    /// The transaction queue read in batches.
    queue: Q,
    /// The engine state records are applied to.
    state: S,
}

/// Input fields for constructing an [`Engine`].
#[derive(Clone, Copy, Debug)]
pub struct EngineParts<Q, S> {
    /// The transaction queue read in batches.
    pub queue: Q,
    /// The engine state records are applied to.
    pub state: S,
}

impl<Q, S> From<EngineParts<Q, S>> for Engine<Q, S> {
    #[inline]
    fn from(parts: EngineParts<Q, S>) -> Self {
        Self {
            queue: parts.queue,
            state: parts.state,
        }
    }
}

/// Failure to apply one transaction to the engine state.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApplyError<S>
where
    S: State,
{
    /// Loading a client's account state failed.
    #[error("loading the client state failed: {0}")]
    LoadClient(S::LoadClientError),
    /// Loading a deposit's lifecycle failed.
    #[error("loading the deposit state failed: {0}")]
    LoadDeposit(S::LoadDepositError),
    /// A referenced deposit's client had no account state.
    #[error("client {client:?} has a deposit but no account state")]
    MissingAccount {
        /// The client whose account state is absent.
        client: ClientId,
    },
    /// Account arithmetic overflowed while applying a transaction.
    #[error("account arithmetic overflowed for client {client:?}")]
    Overflow {
        /// The client whose balance overflowed.
        client: ClientId,
    },
    /// Storing a client's account state failed.
    #[error("storing the client state failed: {0}")]
    StoreClient(S::StoreClientError),
    /// Storing a deposit's lifecycle failed.
    #[error("storing the deposit state failed: {0}")]
    StoreDeposit(S::StoreDepositError),
    /// The transaction kind is not recognised by this engine version.
    #[error("unsupported transaction kind")]
    Unsupported,
}

/// Failure to drain the transaction stream into a balance stream.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConsumeError<Q, S, T>
where
    Q: Queue<Transaction>,
    S: State,
    T: Transactions,
{
    /// Appending a transaction batch to the queue failed.
    #[error("appending a transaction batch failed: {0}")]
    Append(Q::AppendError),
    /// Applying a transaction to the engine state failed.
    #[error(transparent)]
    Apply(#[from] ApplyError<S>),
    /// Reading the next transaction batch failed.
    #[error("reading the next transaction batch failed: {0}")]
    NextBatch(Q::NextBatchError),
    /// The queue offset overflowed.
    #[error("the queue offset overflowed")]
    OffsetOverflow,
    /// The batch length no longer fits the offset type.
    #[error("the batch length no longer fits the offset type: {0}")]
    OffsetWiden(#[from] core::num::TryFromIntError),
    /// Reading the transaction stream failed.
    #[error("reading the transaction stream failed: {0}")]
    Stream(T::Error),
}

impl<Q, S> Engine<Q, S>
where
    Q: Queue<Transaction>,
    S: State,
{
    /// Apply one transaction to the engine state.
    async fn apply(
        &self,
        transaction: Transaction,
        cancel: &CancellationToken,
    ) -> Result<(), ApplyError<S>> {
        match transaction {
            Transaction::Deposit { client, tx, amount } => {
                let value = *amount;
                if self
                    .state
                    .load_deposit(tx, cancel)
                    .await
                    .map_err(ApplyError::LoadDeposit)?
                    .is_none()
                {
                    self.state
                        .store_deposit(
                            tx,
                            DepositState::from(DepositStateParts {
                                amount: value,
                                client,
                                status: DepositStatus::Available,
                            }),
                            cancel,
                        )
                        .await
                        .map_err(ApplyError::StoreDeposit)?;
                }
                let Some(state) = self
                    .state
                    .load_client(client, cancel)
                    .await
                    .map_err(ApplyError::LoadClient)?
                else {
                    self.state
                        .store_client(
                            client,
                            ServiceState::from(ServiceStateParts {
                                available: value,
                                held: Decimal::ZERO,
                                locked: false,
                            }),
                            cancel,
                        )
                        .await
                        .map_err(ApplyError::StoreClient)?;
                    return Ok(());
                };
                let available = state
                    .available()
                    .checked_add(value)
                    .ok_or(ApplyError::Overflow { client })?;
                self.state
                    .store_client(
                        client,
                        ServiceState::from(ServiceStateParts {
                            available,
                            held: state.held(),
                            locked: state.locked(),
                        }),
                        cancel,
                    )
                    .await
                    .map_err(ApplyError::StoreClient)?;
                Ok(())
            }
            Transaction::Withdrawal { client, amount, .. } => {
                let state = if let Some(state) = self
                    .state
                    .load_client(client, cancel)
                    .await
                    .map_err(ApplyError::LoadClient)?
                {
                    state
                } else {
                    let zeroed = ServiceState::from(ServiceStateParts {
                        available: Decimal::ZERO,
                        held: Decimal::ZERO,
                        locked: false,
                    });
                    self.state
                        .store_client(client, zeroed, cancel)
                        .await
                        .map_err(ApplyError::StoreClient)?;
                    zeroed
                };
                if state.available() < *amount {
                    return Ok(());
                }
                let available = state
                    .available()
                    .checked_sub(*amount)
                    .ok_or(ApplyError::Overflow { client })?;
                self.state
                    .store_client(
                        client,
                        ServiceState::from(ServiceStateParts {
                            available,
                            held: state.held(),
                            locked: state.locked(),
                        }),
                        cancel,
                    )
                    .await
                    .map_err(ApplyError::StoreClient)?;
                Ok(())
            }
            Transaction::Dispute { client, tx } => {
                let Some(deposit) = self
                    .state
                    .load_deposit(tx, cancel)
                    .await
                    .map_err(ApplyError::LoadDeposit)?
                else {
                    return Ok(());
                };
                if deposit.client() != client || deposit.status() != DepositStatus::Available {
                    return Ok(());
                }
                let amount = deposit.amount();
                self.state
                    .store_deposit(
                        tx,
                        DepositState::from(DepositStateParts {
                            amount,
                            client: deposit.client(),
                            status: DepositStatus::Disputed,
                        }),
                        cancel,
                    )
                    .await
                    .map_err(ApplyError::StoreDeposit)?;
                let Some(state) = self
                    .state
                    .load_client(client, cancel)
                    .await
                    .map_err(ApplyError::LoadClient)?
                else {
                    return Err(ApplyError::MissingAccount { client });
                };
                let available = state
                    .available()
                    .checked_sub(amount)
                    .ok_or(ApplyError::Overflow { client })?;
                let held = state
                    .held()
                    .checked_add(amount)
                    .ok_or(ApplyError::Overflow { client })?;
                self.state
                    .store_client(
                        client,
                        ServiceState::from(ServiceStateParts {
                            available,
                            held,
                            locked: state.locked(),
                        }),
                        cancel,
                    )
                    .await
                    .map_err(ApplyError::StoreClient)?;
                Ok(())
            }
            Transaction::Resolve { client, tx } => {
                let Some(deposit) = self
                    .state
                    .load_deposit(tx, cancel)
                    .await
                    .map_err(ApplyError::LoadDeposit)?
                else {
                    return Ok(());
                };
                if deposit.client() != client || deposit.status() != DepositStatus::Disputed {
                    return Ok(());
                }
                let amount = deposit.amount();
                self.state
                    .store_deposit(
                        tx,
                        DepositState::from(DepositStateParts {
                            amount,
                            client: deposit.client(),
                            status: DepositStatus::Available,
                        }),
                        cancel,
                    )
                    .await
                    .map_err(ApplyError::StoreDeposit)?;
                let Some(state) = self
                    .state
                    .load_client(client, cancel)
                    .await
                    .map_err(ApplyError::LoadClient)?
                else {
                    return Err(ApplyError::MissingAccount { client });
                };
                let held = state
                    .held()
                    .checked_sub(amount)
                    .ok_or(ApplyError::Overflow { client })?;
                let available = state
                    .available()
                    .checked_add(amount)
                    .ok_or(ApplyError::Overflow { client })?;
                self.state
                    .store_client(
                        client,
                        ServiceState::from(ServiceStateParts {
                            available,
                            held,
                            locked: state.locked(),
                        }),
                        cancel,
                    )
                    .await
                    .map_err(ApplyError::StoreClient)?;
                Ok(())
            }
            Transaction::Chargeback { client, tx } => {
                let Some(deposit) = self
                    .state
                    .load_deposit(tx, cancel)
                    .await
                    .map_err(ApplyError::LoadDeposit)?
                else {
                    return Ok(());
                };
                if deposit.client() != client || deposit.status() != DepositStatus::Disputed {
                    return Ok(());
                }
                let amount = deposit.amount();
                self.state
                    .store_deposit(
                        tx,
                        DepositState::from(DepositStateParts {
                            amount,
                            client: deposit.client(),
                            status: DepositStatus::ChargedBack,
                        }),
                        cancel,
                    )
                    .await
                    .map_err(ApplyError::StoreDeposit)?;
                let Some(state) = self
                    .state
                    .load_client(client, cancel)
                    .await
                    .map_err(ApplyError::LoadClient)?
                else {
                    return Err(ApplyError::MissingAccount { client });
                };
                let held = state
                    .held()
                    .checked_sub(amount)
                    .ok_or(ApplyError::Overflow { client })?;
                self.state
                    .store_client(
                        client,
                        ServiceState::from(ServiceStateParts {
                            available: state.available(),
                            held,
                            locked: true,
                        }),
                        cancel,
                    )
                    .await
                    .map_err(ApplyError::StoreClient)?;
                Ok(())
            }
            Transaction::Eof => Ok(()),
            _ => Err(ApplyError::Unsupported),
        }
    }
}

impl<Q, S, T> Consumer<T> for Engine<Q, S>
where
    Q: Queue<Transaction> + core::fmt::Debug,
    S: State + core::fmt::Debug,
    T: Transactions + core::fmt::Debug,
{
    type Balances = BalanceStream<S>;
    type ConsumeError = ConsumeError<Q, S, T>;

    #[inline]
    async fn consume(
        self,
        cancel: &CancellationToken,
        transactions: &mut T,
    ) -> Result<Self::Balances, Self::ConsumeError> {
        while let Some(batch) = transactions
            .next_batch(cancel)
            .await
            .map_err(ConsumeError::Stream)?
        {
            let _offset = self
                .queue
                .append(batch, cancel)
                .await
                .map_err(ConsumeError::Append)?;
        }
        let _offset = self
            .queue
            .append(&[Transaction::Eof], cancel)
            .await
            .map_err(ConsumeError::Append)?;

        let mut offset = Offset::from(0);
        'apply: loop {
            let batch = self
                .queue
                .next_batch(offset, cancel)
                .await
                .map_err(ConsumeError::NextBatch)?;
            for &transaction in batch {
                if matches!(transaction, Transaction::Eof) {
                    break 'apply;
                }
                self.apply(transaction, cancel).await?;
            }
            let advance = u64::try_from(batch.len())?;
            offset = Offset::from(
                (*offset)
                    .checked_add(advance)
                    .ok_or(ConsumeError::OffsetOverflow)?,
            );
        }

        Ok(BalanceStream {
            state: self.state,
            next: None,
            slot: None,
        })
    }
}
