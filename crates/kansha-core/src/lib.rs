//! Kansha core: accounting engine, persistence, and services.
//!
//! All financial logic lives here (DR-01). This crate has no Tauri
//! dependency; the desktop shell calls into it through thin command
//! handlers.

pub mod accounts;
pub mod audit;
pub mod backup;
pub mod book;
pub mod categories;
pub mod csv;
pub mod date;
pub mod error;
pub mod import;
pub mod integrity;
pub mod invest;
pub mod ledger;
pub mod local_config;
pub mod money;
pub mod persistence;
pub mod reconcile;
pub mod reports;
pub mod sample;
pub mod schedule;
pub mod securities;
pub mod security;
mod serde_impls;
pub mod settings;
pub mod testkit;
mod text_enum;
pub mod undo;

pub use date::{Clock, Date, FixedClock, SystemClock, Timestamp};
pub use error::{Error, Result};
pub use money::{Money, Price, Quantity, Rate, extended_value};
pub use persistence::{Db, Origin, Tx};
