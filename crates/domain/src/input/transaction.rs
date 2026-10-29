//! Transaction variants and their CSV-to-domain mapping.

use crate::client_id::ClientId;
use crate::input::amount::Amount;
use crate::input::tx_id::TxId;

/// The flat, borrowed shape of a CSV row, before validation against the kind.
#[derive(Debug, serde::Deserialize)]
struct RawRecord<'input> {
    /// The amount, absent on rows whose kind carries no amount.
    amount: Option<Amount>,
    /// The client identifier.
    client: u16,
    /// The transaction kind read from the `type` column.
    #[serde(rename = "type")]
    kind: &'input str,
    /// The transaction identifier.
    tx: u32,
}

/// A single transaction in the payments stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(try_from = "RawRecord<'_>")]
#[non_exhaustive]
pub enum Transaction {
    /// A final reversal of a disputed deposit, freezing the account.
    Chargeback { client: ClientId, tx: TxId },
    /// A credit to the client account.
    Deposit {
        client: ClientId,
        tx: TxId,
        amount: Amount,
    },
    /// A claim that a deposit is erroneous, moving its funds to held.
    Dispute { client: ClientId, tx: TxId },
    /// End of the input stream, emitted by the transport once the reader is
    /// exhausted.
    Eof,
    /// A resolution releasing held funds back to available.
    Resolve { client: ClientId, tx: TxId },
    /// A debit from the client account.
    Withdrawal {
        client: ClientId,
        tx: TxId,
        amount: Amount,
    },
}

/// Failure to map a parsed CSV record onto a `Transaction`.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum TransactionTryFromRawError<'input> {
    /// A `deposit` or `withdrawal` row carried no amount.
    #[error("deposit or withdrawal is missing an amount")]
    MissingAmount,
    /// A `dispute`, `resolve`, or `chargeback` row carried an amount.
    #[error("dispute, resolve, or chargeback must not carry an amount")]
    UnexpectedAmount,
    /// The `type` column carried a value matching no variant.
    #[error("unknown transaction type: {0}")]
    UnknownType(&'input str),
}

impl<'input> TryFrom<RawRecord<'input>> for Transaction {
    type Error = TransactionTryFromRawError<'input>;

    #[inline]
    fn try_from(raw: RawRecord<'input>) -> Result<Self, Self::Error> {
        let client = ClientId::from(raw.client);
        let tx = TxId::from(raw.tx);
        match raw.kind {
            "deposit" => {
                let amount = raw
                    .amount
                    .ok_or(TransactionTryFromRawError::MissingAmount)?;
                Ok(Self::Deposit { client, tx, amount })
            }
            "withdrawal" => {
                let amount = raw
                    .amount
                    .ok_or(TransactionTryFromRawError::MissingAmount)?;
                Ok(Self::Withdrawal { client, tx, amount })
            }
            "dispute" => {
                if raw.amount.is_some() {
                    return Err(TransactionTryFromRawError::UnexpectedAmount);
                }
                Ok(Self::Dispute { client, tx })
            }
            "resolve" => {
                if raw.amount.is_some() {
                    return Err(TransactionTryFromRawError::UnexpectedAmount);
                }
                Ok(Self::Resolve { client, tx })
            }
            "chargeback" => {
                if raw.amount.is_some() {
                    return Err(TransactionTryFromRawError::UnexpectedAmount);
                }
                Ok(Self::Chargeback { client, tx })
            }
            unknown => Err(TransactionTryFromRawError::UnknownType(unknown)),
        }
    }
}
