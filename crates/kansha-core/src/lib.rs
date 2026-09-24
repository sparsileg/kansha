//! Kansha core: accounting engine, persistence, and services.
//!
//! All financial logic lives here (DR-01). This crate has no Tauri
//! dependency; the desktop shell calls into it through thin command
//! handlers.

pub mod accounts;
pub mod audit;
pub mod categories;
pub mod date;
pub mod error;
pub mod integrity;
pub mod ledger;
pub mod money;
pub mod persistence;
pub mod sample;
mod serde_impls;
pub mod testkit;
mod text_enum;

pub use date::{Clock, Date, FixedClock, SystemClock, Timestamp};
pub use error::{Error, Result};
pub use money::{Money, Price, Quantity, Rate, extended_value};
pub use persistence::{Db, Origin, Tx};
