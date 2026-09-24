//! SQLite mapping for core value types.
//!
//! Money, quantities, prices, and rates are INTEGER (cents or × 10^6);
//! dates and timestamps are canonical TEXT. A value that does not parse
//! is an error, never a silent default.

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};

use crate::date::{Date, Timestamp};
use crate::money::{Money, Price, Quantity, Rate};

impl ToSql for Money {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(self.cents().into())
    }
}

impl FromSql for Money {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value.as_i64().map(Money::from_cents)
    }
}

macro_rules! scaled_sql {
    ($($t:ty),+) => {$(
        impl ToSql for $t {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(self.raw().into())
            }
        }

        impl FromSql for $t {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                value.as_i64().map(<$t>::from_raw)
            }
        }
    )+};
}

scaled_sql!(Quantity, Price, Rate);

macro_rules! text_sql {
    ($($t:ty),+) => {$(
        impl ToSql for $t {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(self.to_string().into())
            }
        }

        impl FromSql for $t {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                value
                    .as_str()?
                    .parse()
                    .map_err(|e: crate::error::Error| FromSqlError::Other(Box::new(e)))
            }
        }
    )+};
}

text_sql!(Date, Timestamp);

macro_rules! id_sql {
    ($($t:ty),+) => {$(
        impl ToSql for $t {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(self.0.into())
            }
        }

        impl FromSql for $t {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                value.as_i64().map(Self)
            }
        }
    )+};
}

id_sql!(
    crate::accounts::AccountId,
    crate::categories::CategoryId,
    crate::categories::PayeeId,
    crate::categories::TagId
);
