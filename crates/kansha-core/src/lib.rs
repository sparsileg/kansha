//! Kansha core: accounting engine, persistence, and services.
//!
//! All financial logic lives here (DR-01). This crate has no Tauri
//! dependency; the desktop shell calls into it through thin command
//! handlers.

pub mod date;
pub mod error;
pub mod money;

pub use date::{Clock, Date, FixedClock, SystemClock};
pub use error::{Error, Result};
pub use money::{Money, Price, Quantity, extended_value};
