# Phase 2 — Ledger Engine

Date: 2026-09-24. Spec: 0.3.1. No schema change; no IPC change.

## Files

- `ledger/mod.rs`: `Txn`, `Posting`, `TxnInput`, `PostingInput`, `Target`, `TxnSource`, `TxnStatus`, `Cleared`; register view `Entry`/`EntryLine` (`to_input`, `from_txn`, `remainder`); `split_remainder`; reads `get`, `balance`, `cleared_balance`, `category_total`, `register` (running balance), `register_summary` (current, cleared, ending, available credit).
- `ledger/service.rs`: `create`, `create_entry`, `update`, `update_entry`, `void`, `delete`, `set_cleared`, `close_account`; validation.
- `persistence/ledger.rs`: txn/posting/posting_tag SQL; audited insert, update, void, delete, set_cleared; balance and register queries.
- `integrity/mod.rs`, `persistence/integrity.rs`: integrity check v1.
- `testkit.rs`: `Book` test data builders (TEST-110), `EntryBuilder`.
- `persistence/{categories,payees,tags}.rs`: `merge` (CAT-020, PAY-030, TAG-020); `payees::find_or_insert` (PAY-010). `categories::Merged`.
- `persistence/accounts.rs`: linked cash / linked liability rules.
- `accounts/mod.rs`: `is_liability`, `is_cash_bearing`. `money.rs`: `checked_neg`. `error.rs`: `ConfirmationRequired`.
- Tests: `tests/integration/{ledger,integrity}.rs`; ledger property test in `tests/properties.rs`; runner extended in `tests/scenarios.rs`; `tests/scenarios/ledger/LEDGER-001…009.toml`; format in `tests/scenarios/README.md`.

## Decisions

- Engine stores postings; register works with an `Entry` (one account's posting + lines, same sign, must sum). Remainder computed in Rust (TXN-020).
- Non-zero entry needs ≥1 line; no uncategorized postings. Quicken import (MIG, after the prototype) must map blank categories to one.
- Transaction-level tags stored on the viewing account's posting.
- Void: amounts zeroed, postings kept, status `void`; originals in audit. Void can be deleted, not edited. No un-void.
- Edit replaces all postings (posting IDs not stable); reconciled posting keeps its `reconciliation_id` if it stays reconciled on the same account.
- Reconciled: edit/void/delete/un-reconcile needs `confirmed` → else `Error::ConfirmationRequired`. `Reconciled` set only by the reconcile engine or an import-origin write.
- Closed account: no postings after close date; zero balance or confirm. Closed accounts reject any write touching them (new, edit, void, delete, clear).
- One posting per account per transaction (security postings excepted later).
- Investment accounts rejected by the general ledger API until Phase 6.
- Txn source follows `Tx` origin: UI → manual, Import → batch, System → system; Scheduler must use Phase 4 path (`create_with_source`, crate-private).
- Merges: one `merge` audit entry on the source (before = source, after = `Merged` counts). Affected txns get no per-txn audit entry.
- Integrity v1 checks: SQLite integrity, FK check, unbalanced, no account posting, duplicate account posting, void with amount, posting after close, posting linked to unfinished reconciliation, category cycle, category kind mismatch. Transfer-sides check is structural (single txn).
- Scenario runner always runs the integrity check at end; `expect_error` for refusals.

## Known gaps / next

- Register: filters/sort (REG-040), payee/category names in rows, NFR-040 timing — Phase 3.
- `register_rows` uses correlated subqueries per row; measure against 10k txns in Phase 3.
- Close investment account: open-position check — Phase 6.
- Hidden categories/payees/tags still accepted on new postings (UI hides them).
- Integrity: lots/shares and reconciliation-history checks — Phases 5, 6.
- Merges don't write per-transaction audit entries (AUD-020 view of a txn won't show a category merge).
- Un-void (TXN-040) not offered.
