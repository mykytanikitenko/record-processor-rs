//! The per-client account balance row written to CSV.

use rust_decimal::Decimal;

use crate::client_id::ClientId;

/// A per-client account balance row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccountBalance {
    /// The funds available for trading or withdrawal.
    available: Balance,
    /// The client identifier.
    client: ClientId,
    /// The funds held under dispute.
    held: Balance,
    /// Whether the account is frozen.
    locked: bool,
    /// The total of available and held funds.
    total: Balance,
}

/// Input fields for constructing an `AccountBalance`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]

pub struct Parts {
    /// The funds available for trading or withdrawal.
    pub available: Balance,
    /// The client identifier.
    pub client: ClientId,
    /// The funds held under dispute.
    pub held: Balance,
    /// Whether the account is frozen.
    pub locked: bool,
}

/// A balance amount, which may be zero or negative.
#[derive(Clone, Copy, Debug, Eq, PartialEq, derive_more::Deref, serde::Serialize)]
pub struct Balance(Decimal);

/// Failure to construct an `AccountBalance`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ConstructionError {
    /// `available + held` overflowed the decimal range.
    #[error("total overflowed: {available} + {held}")]
    Overflow {
        /// The available balance that overflowed.
        available: Decimal,
        /// The held balance that overflowed.
        held: Decimal,
    },
}

impl serde::Serialize for AccountBalance {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct as _;
        let mut state = serializer.serialize_struct("AccountBalance", 5)?;
        state.serialize_field("client", &self.client)?;
        state.serialize_field("available", &self.available)?;
        state.serialize_field("held", &self.held)?;
        state.serialize_field("total", &self.total)?;
        state.serialize_field("locked", &self.locked)?;
        state.end()
    }
}

impl From<Decimal> for Balance {
    #[inline]
    fn from(value: Decimal) -> Self {
        Self(value)
    }
}

impl TryFrom<Parts> for AccountBalance {
    type Error = ConstructionError;

    /// Constructs an `AccountBalance`, deriving `total` from `available` and
    /// `held` so the invariant `total == available + held` holds by
    /// construction.
    ///
    /// # Errors
    ///
    /// Returns `ConstructionError::Overflow` when `available + held` exceeds
    /// the decimal range.
    #[inline]
    fn try_from(parts: Parts) -> Result<Self, Self::Error> {
        let available = *parts.available;
        let held = *parts.held;
        let total = available
            .checked_add(held)
            .ok_or(ConstructionError::Overflow { available, held })?;
        Ok(Self {
            available: parts.available,
            client: parts.client,
            held: parts.held,
            locked: parts.locked,
            total: Balance::from(total),
        })
    }
}
