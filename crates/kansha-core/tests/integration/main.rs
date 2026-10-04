//! Integration tests against a real SQLite database (TEST-040, TEST-100).
//!
//! Every test starts from `fixture::db()`: a fresh in-memory database with
//! all migrations applied and a fixed clock.

// Test helpers outside #[test] fns panic on setup failure by design.
#![allow(clippy::unwrap_used)]

mod backup;
mod encryption;
mod fixture;
mod import;
mod insights;
mod integrity;
mod invest;
mod ipc_json;
mod ledger;
mod migrations;
mod perf;
mod performance;
mod reconcile;
mod register;
mod reports;
mod repositories;
mod roth_conversion;
mod schedule;
mod schema;
mod true_up;
mod undo;
