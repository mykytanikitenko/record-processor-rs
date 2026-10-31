//! The consumer contract and its production engine.
//!
//! A [`Consumer`] drains its transaction queue, applying each record to the
//! engine [`State`](crate::engine_state::State), and stops at the end-of-stream
//! marker. The concrete [`Type`] dispatches between the production
//! [`Engine`] and a unit-test mock.

use core::error::Error;

use record_processor_domain::client_id::ClientId;
use record_processor_domain::input::Transaction;
use rust_decimal::Decimal;
use storage::offset::Offset;
use storage::queue::Queue;
use tokio_util::sync::CancellationToken;

use crate::engine_state::{
    DepositState, DepositStateParts, DepositStatus, ServiceState, ServiceStateParts, State,
};

/// A payments consumer that drains its transaction queue into engine state.
#[cfg_attr(
    feature = "mocks",
    mockall::automock(type ConsumeError = crate::mocks::MockConsumeError;)
)]
pub trait Consumer {
    /// The error produced by [`Consumer::consume`].
    type ConsumeError: Error;

    /// Drain the transaction queue, applying every record to engine state and
    /// stopping at the end-of-stream marker.
    async fn consume(&self, cancel: &CancellationToken) -> Result<(), Self::ConsumeError>;
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

/// Failure to drain the transaction queue.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConsumeError<Q, S>
where
    Q: Queue<Transaction>,
    S: State,
{
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
                let Some(state) = self
                    .state
                    .load_client(client, cancel)
                    .await
                    .map_err(ApplyError::LoadClient)?
                else {
                    return Ok(());
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

impl<Q, S> Consumer for Engine<Q, S>
where
    Q: Queue<Transaction> + core::fmt::Debug,
    S: State + core::fmt::Debug,
{
    type ConsumeError = ConsumeError<Q, S>;

    #[inline]
    async fn consume(&self, cancel: &CancellationToken) -> Result<(), Self::ConsumeError> {
        let mut offset = Offset::from(0);
        loop {
            let batch = self
                .queue
                .next_batch(offset, cancel)
                .await
                .map_err(ConsumeError::NextBatch)?;
            for &transaction in batch {
                if matches!(transaction, Transaction::Eof) {
                    return Ok(());
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
    }
}

/// Dispatch between the production engine and the unit-test mock.
#[non_exhaustive]
#[derive(Debug)]
pub enum Type<Q, S> {
    /// The unit-test mock.
    #[cfg(feature = "mocks")]
    Mock(MockConsumer),
    /// The production engine.
    Real(Engine<Q, S>),
}

/// Failure to drain a dispatched consumer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DispatchError<Q, S>
where
    Q: Queue<Transaction>,
    S: State,
{
    /// The unit-test mock failed.
    #[cfg(feature = "mocks")]
    #[error("mocked consumer failed: {0}")]
    Mock(crate::mocks::MockConsumeError),
    /// The production engine failed.
    #[error(transparent)]
    Real(ConsumeError<Q, S>),
}

impl<Q, S> Consumer for Type<Q, S>
where
    Q: Queue<Transaction> + core::fmt::Debug,
    S: State + core::fmt::Debug,
{
    type ConsumeError = DispatchError<Q, S>;

    #[inline]
    async fn consume(&self, cancel: &CancellationToken) -> Result<(), Self::ConsumeError> {
        match self {
            #[cfg(feature = "mocks")]
            Self::Mock(mock) => mock.consume(cancel).await.map_err(DispatchError::Mock),
            Self::Real(engine) => engine.consume(cancel).await.map_err(DispatchError::Real),
        }
    }
}
