# Phase 1 — Schema and Persistence

Date: 2026-09-24. Spec: 0.3.

## Files

- `crates/kansha-core/src/persistence/migrations/0001_init.sql`: full schema (spec §18 points here). 21 tables + `sqlite_sequence`, STRICT, CHECKs, FKs, indexes, `unbalanced_txn` view, append-only audit triggers, 11 seeded system categories, `application_id` 'KNSH'.
- `persistence/mod.rs`: `Db` (open file / in-memory, pragmas verified), `Db::write` (IMMEDIATE txn, commit on Ok), `Tx` (timestamp + origin), `Origin`.
- `persistence/migrate.rs`: forward-only runner; refuses newer schema, non-Kansha files, and gaps in `schema_version`; `migrate_to` for TEST-100.
- `persistence/audit.rs`: `record`, `history`.
- `persistence/{accounts,categories,payees,tags,settings}.rs`: first repositories.
- `persistence/values.rs`: SQL mapping for Money/Quantity/Price/Rate/Date/Timestamp and IDs.
- `accounts/mod.rs`, `categories/mod.rs`: domain types.
- `text_enum.rs`: TEXT enums (CHECK list ↔ Rust enum). `serde_impls.rs`: values → JSON strings.
- `date.rs`: `Timestamp`; `Clock::now()`. `money.rs`: `Rate`. `error.rs`: DB and domain variants.
- `tests/integration/`: fixture, migrations (TEST-040, TEST-100), schema constraints, repositories.

## Decisions

- D-50 per-account MMF mode; D-60 lot methods fifo/specific/average/hifo/min_tax in the schema; D-100 as listed; D-110 unencrypted prototype. D-10 deferred to Phase 3 (no schema impact).
- `txn` table name. Equity category kind for opening balances. Holdings as security-tagged postings at cost basis. Lots are immutable; open quantity/basis derived from disposals and adjustments. Account type fixed at creation.
- rusqlite 0.40, `bundled-sqlcipher-vendored-openssl` (no system OpenSSL on any platform).

## Known gaps / next

- Repositories for txn/posting, securities, lots, schedules, reconciliation, import batches: Phase 2+ as each engine arrives.
- Referential rules needing other rows (linked cash account must be a cash-bearing open account; zero balance to close): Phase 2 services.
- Backup before migration (BAK-020): Phase 8.
- Windows CI build of vendored OpenSSL not yet confirmed.
