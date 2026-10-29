//! The streaming payments engine: applies transactions to account state.

use std::collections::BTreeMap;

use domain::client_id::ClientId;
use domain::input::{Transaction, TxId};
use domain::output::{AccountBalance, Balance, OverflowError, Parts};
use rust_decimal::Decimal;

use crate::service::{Service, TransactionStream};

/// Per-client account state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Account {
    /// Funds available for trading or withdrawal.
    available: Decimal,
    /// Funds held under dispute.
    held: Decimal,
    /// Whether the account is frozen after a chargeback.
    locked: bool,
}

/// Failure to apply a single transaction to the engine state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ApplyError {
    /// Account arithmetic overflowed while applying a transaction.
    #[error("account arithmetic overflowed for client {client:?}")]
    Overflow {
        /// The client whose balance overflowed.
        client: ClientId,
    },
    /// The transaction kind is not recognised by this engine version.
    #[error("unsupported transaction kind")]
    Unsupported,
}

/// Failure to build the final account balances.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum BalancesError {
    /// The total overflowed while constructing a balance.
    #[error(transparent)]
    Overflow(#[from] OverflowError),
}

/// A recorded deposit, referenced by later dispute, resolve, or chargeback
/// actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Deposit {
    /// The deposited amount.
    amount: Decimal,
    /// The client the deposit was credited to.
    client: ClientId,
    /// The current lifecycle stage.
    status: DepositStatus,
}

/// The lifecycle of a recorded deposit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DepositStatus {
    /// The deposit is intact and its funds are available.
    Available,
    /// The deposit was reversed by a chargeback and its funds are gone.
    ChargedBack,
    /// The deposit is under dispute and its funds are held.
    Disputed,
}

/// The streaming payments engine.
#[derive(Debug)]
pub struct Engine {
    /// Per-client account state.
    accounts: BTreeMap<ClientId, Account>,
    /// Recorded deposits, keyed by transaction identifier.
    deposits: BTreeMap<TxId, Deposit>,
}

/// Failure to serve a transaction stream.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServeError<E> {
    /// Applying a transaction to the engine state failed.
    #[error(transparent)]
    Apply(#[from] ApplyError),
    /// Building the final account balances failed.
    #[error(transparent)]
    Balance(#[from] BalancesError),
    /// The input stream failed before producing every transaction.
    #[error("input stream failed: {0:?}")]
    Input(E),
}

impl From<()> for Engine {
    #[inline]
    fn from((): ()) -> Self {
        Self {
            accounts: BTreeMap::new(),
            deposits: BTreeMap::new(),
        }
    }
}

impl Engine {
    /// Return the mutable account for `client`, inserting an empty one when
    /// absent.
    fn account_mut(&mut self, client: ClientId) -> &mut Account {
        self.accounts.entry(client).or_insert(Account {
            available: Decimal::ZERO,
            held: Decimal::ZERO,
            locked: false,
        })
    }

    /// Apply a single transaction to the engine state.
    fn apply(&mut self, transaction: Transaction) -> Result<(), ApplyError> {
        match transaction {
            Transaction::Deposit { client, tx, amount } => {
                let value = *amount;
                let _deposit = self.deposits.entry(tx).or_insert(Deposit {
                    amount: value,
                    client,
                    status: DepositStatus::Available,
                });
                let account = self.account_mut(client);
                account.available = account
                    .available
                    .checked_add(value)
                    .ok_or(ApplyError::Overflow { client })?;
                Ok(())
            }
            Transaction::Withdrawal { client, amount, .. } => {
                let Some(account) = self.accounts.get_mut(&client) else {
                    return Ok(());
                };
                if account.available < *amount {
                    return Ok(());
                }
                account.available = account
                    .available
                    .checked_sub(*amount)
                    .ok_or(ApplyError::Overflow { client })?;
                Ok(())
            }
            Transaction::Dispute { client, tx } => {
                let Some(deposit) = self.deposits.get_mut(&tx) else {
                    return Ok(());
                };
                if deposit.client != client || deposit.status != DepositStatus::Available {
                    return Ok(());
                }
                let amount = deposit.amount;
                deposit.status = DepositStatus::Disputed;
                let account = self.account_mut(client);
                account.available = account
                    .available
                    .checked_sub(amount)
                    .ok_or(ApplyError::Overflow { client })?;
                account.held = account
                    .held
                    .checked_add(amount)
                    .ok_or(ApplyError::Overflow { client })?;
                Ok(())
            }
            Transaction::Resolve { client, tx } => {
                let Some(deposit) = self.deposits.get_mut(&tx) else {
                    return Ok(());
                };
                if deposit.client != client || deposit.status != DepositStatus::Disputed {
                    return Ok(());
                }
                let amount = deposit.amount;
                deposit.status = DepositStatus::Available;
                let account = self.account_mut(client);
                account.held = account
                    .held
                    .checked_sub(amount)
                    .ok_or(ApplyError::Overflow { client })?;
                account.available = account
                    .available
                    .checked_add(amount)
                    .ok_or(ApplyError::Overflow { client })?;
                Ok(())
            }
            Transaction::Chargeback { client, tx } => {
                let Some(deposit) = self.deposits.get_mut(&tx) else {
                    return Ok(());
                };
                if deposit.client != client || deposit.status != DepositStatus::Disputed {
                    return Ok(());
                }
                let amount = deposit.amount;
                deposit.status = DepositStatus::ChargedBack;
                let account = self.account_mut(client);
                account.held = account
                    .held
                    .checked_sub(amount)
                    .ok_or(ApplyError::Overflow { client })?;
                account.locked = true;
                Ok(())
            }
            Transaction::Eof => Ok(()),
            _ => Err(ApplyError::Unsupported),
        }
    }

    /// Build the final account balances, ordered by client identifier.
    fn balances(&self) -> Result<Vec<AccountBalance>, BalancesError> {
        let mut rows = Vec::with_capacity(self.accounts.len());
        for (client, account) in &self.accounts {
            rows.push(AccountBalance::try_from(Parts {
                client: *client,
                available: Balance::from(account.available),
                held: Balance::from(account.held),
                locked: account.locked,
            })?);
        }
        Ok(rows)
    }
}

impl<S: TransactionStream> Service<S> for Engine {
    type Error = ServeError<S::Error>;

    #[inline]
    async fn serve(mut self, mut input: S) -> Result<Vec<AccountBalance>, Self::Error> {
        loop {
            match input.next().await {
                Ok(Some(transaction)) => self.apply(transaction)?,
                Ok(None) => break,
                Err(error) => return Err(ServeError::Input(error)),
            }
        }
        Ok(self.balances()?)
    }
}
