# Phase 5 — Reconciliation

Spec: 0.3.9. Engine, IPC, and UI in one pass. `just check` green.

**Exit criteria met on Kubuntu (2026-09-24):** reconciliation scenarios pass; Stan reconciled Checking, Savings, and Visa against the sample August 2026 statements in the app. `just test` on Windows not yet run.

**⚠ API change:** 11 new commands (below); `just bindings` run.
**No schema change.** The 0001 schema already had `reconciliation` and `posting.reconciliation_id`.

## Files

- `reconcile/mod.rs`: types (`Reconciliation`, `ReconStatus`, `StartInput`, `StatementItem`, `Item`, `Session`, `OpeningCheck`, `ChangedTxn`, `HistoryRow`), reads (`get`, `open_for`, `session`, `opening_check`, `history`, `history_items`), change detection from the audit log.
- `reconcile/service.rs`: `start`, `update_statement`, `set_checked`, `add_adjustment`, `finish`, `abandon`.
- `persistence/reconcile.rs`: SQL for all of it, audited writes.
- `integrity`: new check `ReconciledBalanceMismatch` (INT-030 "reconciled balances match reconciliation history").
- `src-tauri/src/commands/reconcile.rs`: `reconcile_open`, `reconcile_opening_check`, `reconcile_start`, `reconcile_session`, `reconcile_update`, `reconcile_check`, `reconcile_adjust`, `reconcile_finish`, `reconcile_abandon`, `reconcile_history`, `reconcile_history_items`.
- UI: `views/Reconcile.svelte`; `lib/state/reconcile.svelte.ts`; `lib/reconcile/form.ts`. Tools > Reconcile and the nav-bar Reconcile button are live; the account view has a Reconcile button.
- Tests: `tests/integration/reconcile.rs` (7, incl. synthetic data month by month and against mock statements), property test `chained_reconciliations_…` in `tests/properties.rs`, scenarios `tests/scenarios/reconcile/RCN-001…011.toml`, enum test in `schema.rs`; frontend `Reconcile.test.ts`, `reconcile.test.ts`, `form.test.ts`.
- Scenario runner: `reconcile_*` actions, `expect.reconcile`, `expect.reconcile_history`, `expect.integrity` (documented in the scenarios README).

## Decisions

- Check marks are `posting.cleared`. Save/resume needs nothing extra; abandon keeps marks.
- `difference = statement − (Σ reconciled + Σ checked ≤ statement date)`. Reconciled total is computed live, so a confirmed edit of a reconciled item mid-session cannot make the math stale.
- `reconciliation.opening_balance` = last finished statement's ending balance (Σ reconciled if none).
- Change detection (RCN-030) reads `audit_log`: entries after the audit entry that finished the last statement, compared per transaction end to end. Memo-only edits are not listed. Finish writes one audit entry on the reconciliation, not per transaction.
- Statement dates cannot precede the last finished one.
- Reconcilable: checking, savings, cash, money market, credit card. Rule lives in Rust (`is_reconcilable`); the UI has a copy of the list for the account picker only.
- Statement sign (spec 0.3.9): reconcile commands take and return amounts as the statement prints them. On a credit card the balance owed is positive, charges positive, payments negative. `reconcile::Sign` converts at the module boundary; storage, the register, and the rest of the engine stay in ledger sign.
- Statement items: positive amount from the user; engine picks the sign (interest on a liability is a charge). Created cleared, source `reconcile`.
- Balance Adjustment: built-in category, needs `confirmed`, only when the difference is non-zero.
- `expect.integrity` in scenarios lets a scenario assert a failing check; without it the check must be clean.

- Sample data (TEST-110) arrives reconciled: one finished statement per month end before the statement month (the month before today's) for every reconcilable account; later entries unmarked, none pre-checked. The Auto Loan does not reconcile; its old entries stay cleared.
- Sample Checking never goes negative before today: pay raised to 2,850.00, and a sweep on the 25th keeps 4,000.00 (grown yearly) in Checking, excess to Savings, shortfall back from Savings.
- Sample Visa gets extra charges in the statement month; a monthly check (Green Lawn Co, 27th) gives Checking an outstanding item.
- `sample::statement` builds a mock bank statement: open items the bank has posted (checks after 7 days, bank interest at once, others after 2), ending balance, and outstanding items. `cargo run -p kansha-core --example sample_statements [today] [seed]` prints Checking, Savings, and Visa statements matching the app's "Load sample data" on that day.

## Found while testing

- Reconcile view in a session: statement, totals, and buttons stay put; each item list scrolls on its own with a sticky header. Checkbox sits by the amount, is larger, and a click anywhere on the row toggles it. The row holding focus is highlighted like a focused field.
- Svelte `$effect` that reset the start form read and wrote the same state and looped; fixed with `untrack`. Caught by `Reconcile.test.ts`.

## UI changes after the exit check (spec 0.3.10)

- Date format setting (SET-030): `state/dateformat.svelte.ts`; `displayDate`/`parseDate` follow it; placeholders use `datePattern()`, messages `dateExample()`. Settings dialog has the choice. localStorage until SET-070.
- Focus colors are theme variables on `.app` (`--focus-bg`, `--focus-fg`, `--focus-sel-bg`, `--focus-ring`, `--sel-bg`, `--sel-fg`, `--opt-bg`, `--opt-fg`); fields of every kind, buttons, the reconcile focus row, menus, dropdown lists, and text selection use them. Dark theme: white on deep blue with a yellow ring; `color-scheme` set per theme so native controls match. The ring is drawn inside the field's edge (a neighbouring register cell covered it outside), and a focused field's selected text keeps the focus text color on `--focus-sel-bg` (the general selection colors had replaced the focus look when tabbing).
- Select-on-focus is app-wide: `ui/selectOnFocus.ts` is installed once on `document` by `App.svelte` (the per-field action is gone).
- Register filter bar: inline labels, boxes sized to content, no stretching; wraps only when the window is too narrow.

## Known gaps

- `just test` not yet run on Windows (phase-end rule, spec §24).
- No "mark all" beyond the header checkbox; no keyboard shortcuts beyond Space on a checkbox.
- Reconciliation history is a table in the Reconcile view, not a tab on the account view (UI-030).
- Investment cash reconciliation (RCN-010) and share reconciliation (RCN-070) wait for Phase 6.
- Dashboard "unreconciled beyond threshold" warning (DSH-030): Phase 7.
- Finish and Adjust do not audit per transaction, so AUD-020 history of a transaction does not show being reconciled.
- Out-of-order statements (older than the last) are refused, not supported.
- Register opens on the last page, which can hold only a few rows (e.g. 3 of 803). Decided (Stan, 2026-09-24): remove pagination; the register lazy-scrolls through all transactions. Not yet scheduled; must still meet NFR-040.
- Engine error messages still show dates as ISO (`2026-08-31`), not in the user's format.
- Changing the ending balance after adding a Balance Adjustment does not remove the adjustment; delete it in the register.

## Local test data (not in the repo)

On 2026-09-24 Stan's prototype database was reshaped for reconcile testing: Checking transactions before 2026-05-01 deleted, 8,000.00 opening balance added 2026-04-30, and a compensating Opening Balance entry (2026-04-30) added to Savings, Visa, Cash, and Auto Loan so their balances from 4/30 on are unchanged (their earlier history is skewed). Backup: `kansha.db.before-rebase-2026-09-24` next to `kansha.db`. A practice statement (Aug 2026, ending 3,754.37) is in `~/Downloads/Sample-Bank-Checking-Statement-2026-08.pdf`. The 2026-08-15 deposit "missing from the reconcile list" could not be reproduced: the engine returned it (unchecked) and counts matched the database. Suspected cause was list length; recheck with the shorter data.

Superseded by the reworked sample data: the hands-on reconcile used a fresh scratch book (`KANSHA_DB=/tmp/kansha-scratch.db just dev`, Load sample data) and statements from `cargo run -p kansha-core --example sample_statements`. The item lists now scroll on their own, which addresses the suspected list-length cause above.
