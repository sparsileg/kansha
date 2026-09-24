//! Kansha core: accounting engine, persistence, and services.
//!
//! All financial logic lives here (DR-01). This crate has no Tauri
//! dependency; the desktop shell calls into it through thin command
//! handlers.

pub mod accounts;
pub mod categories;
pub mod date;
pub mod error;
pub mod money;
pub mod persistence;
mod serde_impls;
mod text_enum;

pub use date::{Clock, Date, FixedClock, SystemClock, Timestamp};
pub use error::{Error, Result};
pub use money::{Money, Price, Quantity, Rate, extended_value};
pub use persistence::{Db, Origin, Tx};
