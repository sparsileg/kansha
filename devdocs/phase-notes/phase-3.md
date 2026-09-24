# Phase 3 — Register UI

Spec: 0.3.4. Split into 3a (core queries, sample data, IPC) and 3b (Svelte UI), agreed with Stan.

## Status

| Sub-phase | State |
|---|---|
| 3a Core queries, sample data, IPC | Done. `just check` green. |
| 3b Svelte UI | Not started. |

Phase 2 and 3a are uncommitted in the working tree; Stan commits in GitKraken.

## 3a — done

No schema change. **⚠ API change:** 28 new IPC commands; `src/lib/types/bindings.ts` regenerated.

### Files

- `audit/mod.rs`: `history`, `describe` (per-field changes for AUD-020).
- `ledger/mod.rs`: `RegisterQuery`, `RegisterSort`, `RegisterPage`, `RegisterRow` (payee name, category text, `future` flag), `register_query`, `AccountBalance`, `account_balances`. `Target`, `Counterpart`, `TxnSource` are now tagged with `kind`.
- `ledger/service.rs`: `memorize_payee`.
- `persistence/ledger.rs`: register SQL (window-function running balance, filters, sort, paging). `persistence/payees.rs`: `search`.
- `sample.rs`: synthetic dataset (`SampleSpec`, `SampleSpec::around`, `generate`).
- `serde_impls.rs`, `text_enum.rs`: Deserialize and specta support. New `specta` feature on kansha-core.
- `src-tauri/src/state.rs`: `AppState`, `IpcError`, `ErrorKind`. `src-tauri/src/commands/{accounts,lists,ledger,sample}.rs`. DB at the app data dir, or `KANSHA_DB`.
- Tests: `integration/register.rs` (filters, sort, paging, QuickFill, audit view, NFR-040), `integration/ipc_json.rs`, unit tests in `audit` and `sample`, `src-tauri` `bindings_are_up_to_date`.
- `just bindings` now regenerates the file. The old recipe only built and never wrote it.

### IPC commands available to the UI

- General: `app_version`, `today`.
- Accounts: `account_list`, `account_balances`, `account_create`, `account_update`, `account_close`, `account_reopen`, `account_delete`.
- Lists: `category_list`, `category_create`, `payee_list`, `payee_search`, `payee_update`, `tag_list`, `tag_create`.
- Ledger: `register_query`, `register_summary`, `entry_get`, `entry_create`, `entry_update`, `txn_void`, `txn_delete`, `txn_set_cleared`, `split_remainder`, `audit_history`, `integrity_check`.
- Sample: `sample_data_load`.
- Errors: `Result<T, IpcError>`. `kind = confirmation_required` means ask the user, then repeat with `confirmed = true`.

### Decisions

- Core types cross IPC directly through specta derives behind a feature. No DTO copies.
- Money, quantity, price, rate, date, and timestamp cross as canonical strings. IDs and counters are i64 in Rust and `number` in TS (`dangerously_cast_bigints_to_number`). Money is never an integer on the wire.
- Tagged unions use `{kind, id}` (`Target`, `Counterpart`). Enums are snake_case strings.
- Payee names travel with the entry (`payee_name`). The payee is found or created in the same transaction.
- Memorize payee defaults on the first save only. Never overwrite existing defaults silently (AUD-030).
- The UI does no money math. `split_remainder` is a command.
- Running balance is computed over the whole account before filters and sort, so it is always the true balance in date order (REG-020).
- Category filter includes subcategories and any split line.
- Sample data: 5 accounts, 41 categories, about 50 payees, 3 years, ending 3 weeks after today. `sample_data_load` refuses a book that already has accounts (D-130).
- Old audit JSON for `Target` changed shape. No released data exists, so no migration.

### NFR-040 measured

10,232-row register: 110 ms release, 430 ms debug (all rows). First page (100 rows, newest first): 92 ms release. Text filter: 121 ms release. The test asserts under 1 s in debug, so it may flake on slow CI.

### Known gaps from 3a

- The window function scans the whole account per query. Fine at 10k rows; revisit at 100k+.
- No update, delete, or merge commands for categories and tags. Payee update exists. Add if the UI needs them.
- Payee search is prefix only.

## 3b — remaining sub-tasks

Order is roughly the dependency order. Each item is one reviewable chunk.

1. **API wrapper.** In `src/lib/api/`, unwrap `{status, data|error}` into thrown `ApiError` carrying `IpcError.kind`. Helper for the `confirmation_required` retry flow. Mockable for tests.
2. **Format module** (`src/lib/format/`). Parse typed money ("1,234.56", "-5", ".5") into canonical strings using string operations only. Display money with separators. Date parse and display, and `+`/`-`/`t` date keys via string or Rust helpers, with no JS `Date`. Split negative money into Payment and Deposit columns. Vitest (TEST-120).
3. **State modules** (`src/lib/state/`). Accounts and balances, categories, tags, current account, register query and page, settings. Runes in `.svelte.ts`.
4. **Shell.** View navigation to an Account view. Account selector dropdown that can toggle to a sidebar (UI-010). Groups per ACCT-240. Balances with credit and liability sign display. "Load sample data" button for an empty book, with the empty-book state.
5. **Account modal** (ACCT-200 … ACCT-220). Create and edit. Type-specific fields (interest rate, credit limit, investment settings, other asset). Masked account number with reveal (ACCT-150). Close with confirmation. Reopen. Delete only when empty.
6. **Register grid** (REG-010, REG-020, REG-070). Custom grid with columns Date, Num, Payee, Payment, Deposit, Category, Tag, Memo, Clr, Balance. Virtualized rows or paging (NFR-040). Newest-first opening. Sort by column. Split rows show "--Split--" (REG-050). Today line and future-row styling. Footer (REG-060).
7. **Filters** (REG-040). Date range, payee, category, tag, cleared status, text search. Debounced. Clear-all.
8. **Keyboard entry** (REG-030). Inline entry row at the bottom. Tab order, Enter saves, Esc cancels, `+`/`-` adjusts date, `t` sets today. Full keyboard operation (UI-050).
9. **QuickFill** (PAY-020). Payee autocomplete from `payee_search`. Filling category, tag, memo, and amount from memorized defaults. Sending `payee_name` for new payees.
10. **Splits and transfers** (TXN-020, TXN-030). Split editor with live remainder from `split_remainder`. Save blocked until the remainder is zero. Transfer target picker. "Go to other side" of a transfer.
11. **Edit, void, delete, clear.** Edit in place. Context menu (REG-080). Clr toggle. Confirmation dialog driven by `confirmation_required`.
12. **Audit view** (AUD-020). Per-transaction history panel from `audit_history`, showing changed fields.
13. **Integrity check** button and result list (INT-030), so Stan can run it on generated data.
14. **Tests** (TEST-120). Money and date input parsing, register keyboard behavior, split-remainder validation. IPC mocked.
15. **Payee editor** (PAY-020, PAY-030). Edit a payee's memorized defaults, rename, hide. Needs `payee_merge` and `payee_delete` commands (core has them).
16. **Category and tag management** (CAT-020, CAT-030, CAT-040, TAG-020). Categories: create, rename, re-parent, merge, hide, tax-related, tithable, and giving flags; built-in categories are protected. Tags: create, rename, merge, hide. Delete only when unused. Needs `category_update`, `category_delete`, `category_merge`, `tag_update`, `tag_delete`, `tag_merge` commands (core repositories already have them), then `just bindings`. **⚠ API change.**
17. **Docs and close-out.** Update this file with the final 3b decisions. Bump the spec if it or the conventions change.

### Exit criteria (spec §24, Phase 3)

- Stan enters a month of transactions by keyboard.
- The generator loads a multi-year dataset (done in 3a).
- The register meets NFR-040 in the running app (measured in 3a at the query level; confirm in the UI).

### Decided with Stan

- **D-10:** separate Payment and Deposit columns. The UI splits the signed amount for display (`src/lib/format/`). The entry row has both fields; typing in one clears the other. Recorded in spec 0.3.3.
- **Today line** (REG-070): a horizontal line across the full register row between the last entry dated today or earlier and the first future-dated one. Future rows are dimmed. Stan had no preference; full-width chosen.
- **Category, tag, and payee screens:** the spec's phase table (§24) gives them no phase. Stan wants all three in 3b. Payee editing (memorized defaults, rename, hide, merge), category management (create, rename, re-parent, merge, hide), and tag management (create, rename, merge, hide) are required 3b items 15 and 16.
