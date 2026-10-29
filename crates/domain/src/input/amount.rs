//! A monetary amount bounded to four decimal places.

use rust_decimal::Decimal;

/// A monetary amount strictly greater than zero with at most four decimal
/// places.
#[derive(Clone, Copy, Debug, Eq, PartialEq, derive_more::Deref, serde::Deserialize)]
#[serde(try_from = "Decimal")]
pub struct Amount(Decimal);

/// Failure to construct an `Amount` from a `Decimal`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ConstructionError {
    /// The value is zero or negative.
    #[error("amount {value} is not positive")]
    NonPositive {
        /// The offending value.
        value: Decimal,
    },
    /// The value has more than four fractional digits.
    #[error("amount {value} has more than four decimal places")]
    TooManyDecimalPlaces {
        /// The offending value.
        value: Decimal,
    },
}

impl TryFrom<Decimal> for Amount {
    type Error = ConstructionError;

    /// Constructs an `Amount`, rejecting non-positive values and values with
    /// more than four decimal places after normalization.
    ///
    /// # Errors
    ///
    /// Returns `ConstructionError::NonPositive` for zero or negative values
    /// and `ConstructionError::TooManyDecimalPlaces` when the normalized value
    /// still has more than four fractional digits.
    #[inline]
    fn try_from(value: Decimal) -> Result<Self, Self::Error> {
        let normalized = value.normalize();
        if normalized <= Decimal::ZERO {
            return Err(ConstructionError::NonPositive { value });
        }
        if normalized.scale() > 4 {
            return Err(ConstructionError::TooManyDecimalPlaces { value });
        }
        Ok(Self(normalized))
    }
}
