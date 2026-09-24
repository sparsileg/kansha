# Phase 3 — Register UI

Spec: 0.3.4. Split into 3a (core queries, sample data, IPC) and 3b (Svelte UI), agreed with Stan.

## Status

| Sub-phase | State |
|---|---|
| 3a Core queries, sample data, IPC | Done. `just check` green. |
| 3b Svelte UI | Built. `just check` green (73 frontend tests). **Not run in the real app**: Stan must run `just dev` and check the exit criteria below. |

3a is committed. All of 3b is uncommitted in the working tree; Stan commits in GitKraken.

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

## 3b — what was built

**⚠ API change** (items 5–16, `just bindings` run): new commands `account_defaults`, `account_number_masked`, `category_update`, `category_delete`, `category_merge`, `payee_delete`, `payee_merge`, `tag_update`, `tag_delete`, `tag_merge`; `RegisterRow` gains `tags` (comma-separated tag names of the transaction). `Merged` now derives `specta::Type`. No schema change. Engine change: register query returns tag names (test added in `integration/register.rs`; register tests still pass at NFR-040 timings).

### Items 1–2 (API wrapper, format module)

- `src/lib/api/index.ts`: `commands` (raw generated surface), `call` (unwraps `{status, data|error}`, throws `ApiError` with `kind` and `needsConfirmation`), `withConfirmation(run, ask)` (repeats with `confirmed = true` after the user agrees; returns `null` if declined; rethrows other errors without asking).
- `src/lib/format/money.ts`: `parseMoney`, `formatMoney`, `negateMoney`, `splitPaymentDeposit`, `combinePaymentDeposit`. String operations only.
- `src/lib/format/date.ts`: `addDays`, `parseDate`, `displayDate`, `isValidIso`, `applyDateKey`. Integer calendar math on ISO strings; no JS `Date`. "Today" is always passed in.
- Tests: `src/lib/api/api.test.ts` (5), `src/lib/format/format.test.ts` (24).

### Item 3 (state modules)

- `src/lib/state/lists.svelte.ts`: `listsState` holds `today`, accounts, balances, categories, tags, with id lookup maps, `isEmptyBook`, `loadAll`, `loadBalances`.
- `src/lib/state/register.svelte.ts`: `registerState` holds the open account, filters, sort, page index, page, and summary. Methods: `open`, `close`, `reload`, `refresh` (rows, footer, balances), `setFilters`, `clearFilters`, `sortBy`, `goToPage`.
- `src/lib/state/settings.svelte.ts`: `settingsState.pageSize` (100). Theme and font size stay in `theme.svelte.ts`. Not persisted yet (SET-070).
- Tests: `src/lib/state/state.test.ts` (7), IPC mocked.

### Item 4 (shell)

- `src/App.svelte`: loads `listsState` on mount; nav bar with the account selector (dropdown mode), a dropdown/sidebar toggle, and a theme toggle; error banner; empty-book state.
- `src/lib/components/AccountSelector.svelte`: native `<select>` with group `optgroup`s and current balance, or a sidebar list. `AccountBalance.svelte`: formatted balance, red when negative.
- `src/lib/state/groups.ts`: `groupAccounts` (fixed group order, then `sort_order`, then name; hides closed and `show_in_list = false`). Test in `groups.test.ts`.
- `src/views/Account.svelte`: placeholder header (name, current, ending, entry count). The grid arrives in item 6. `src/views/EmptyBook.svelte`: "Load sample data" (seed 1).
- `settingsState` gains `accountNav` and `showClosedAccounts`.

### Decisions (item 4)

- Balances show as stored: negative means owed on credit and liability accounts, shown in red. No sign flip. Revisit if Stan wants Quicken-style positive "owed" display (`negateMoney` exists).
- Reordering accounts (ACCT-240) waits for the account modal (`sort_order` field), item 5. There is no drag-and-drop yet.
- No toggle in the UI for `showClosedAccounts` yet.
- Not run in the real app: only `just check` (svelte-check and Vitest). No component tests.

### Decisions

- Opening an account resets filters and shows newest-first (date, descending). A filter, sort, or clear-all resets to page 0.
- Sorting a new column starts ascending, except date, which starts descending.
- A response that arrives after a newer request is dropped (stale-response guard by sequence number).
- Current account lives in `registerState.accountId`. `viewState` still only holds the top-level view.
- Errors are stored as message strings on the state (`error`); the shell will display them.

### Decisions (items 1–2)

- `parseMoney` rejects more than two decimals; it never rounds. It accepts thousands commas, a leading `$`, `+`/`-`, and `.5`. "-0" becomes "0.00".
- `parseDate` accepts `M/D/YYYY`, `M/D/YY` (read as 20yy), `M/D` (year from `today`), and ISO. Separators `/`, `.`, `-`. Display is `MM/DD/YYYY`.
- Date keys: `+` or `=` next day, `-` previous day, `t` today. An empty or invalid field falls back to `today` before adjusting.
- `combinePaymentDeposit` returns `null` if both fields are set or either is invalid. The entry row must clear the other field on typing (D-10).
- Not yet wired: no component uses these modules.

## 3b — sub-task status

All 17 items are built. Items 1–4 are described above. Items 5–17:

### Items 5–16

- **5 Account modal** (`AccountModal.svelte`): create and edit; type-specific fields (interest rate, credit limit, investment settings, other asset); type defaults come from Rust (`account_defaults`), so the UI does not duplicate group and tax rules. Account number masked through `account_number_masked` with Reveal (ACCT-150). Close (confirmation via `withConfirmation`), reopen, delete. Type is locked after creation. Reached from "New account" and "Edit account".
- **6 Register grid** (`RegisterGrid.svelte`): Date, Num, Payee, Payment, Deposit, Category, Tag, Memo, Clr, Balance. Paging (100 rows, "Newer/Older"), not virtualized. Newest-first opening. Sort by clicking a header (Payment and Deposit both sort by amount). Split rows show `--Split--` from the query. Today line (REG-070) where `future` flips between adjacent rows, drawn only on date sort. Future rows dimmed. Void rows struck through. Footer: current, cleared, ending, available credit (REG-060), entry count, paging.
- **7 Filters** (`FilterBar.svelte`): date range, payee, category, tag, cleared, text (250 ms debounce), Clear all.
- **8 Keyboard entry** (`EntryEditor.svelte`, `register/draft.ts`, `register/keys.ts`): the entry row is pinned below the grid. Tab order is Date, Num, Payee, Payment, Deposit, Category, Tag, Memo, Enter. Enter saves; Esc cancels. In Date: `+` or `=` next day, `-` previous day, `t` today. After a save the row resets, keeps the date, and refocuses Date. Grid keys: arrows, Home, End, PageUp, PageDown move; Enter edits; Delete deletes; Space toggles cleared; Insert or Ctrl+N focuses the entry row; Shift+F10 or the menu key opens the context menu.
- **9 QuickFill**: payee suggestions from `payee_search` through a datalist. On change, an exact name match fills empty category, tag, memo, and amount from the payee's defaults. Never overwrites typed values. Edits do not QuickFill. `payee_name` is always sent; Rust finds or creates the payee.
- **10 Splits and transfers**: choosing `--Split--` opens the split panel (two blank lines). Line amounts are typed as magnitudes and signed like the total (payment or deposit). The remainder comes from `split_remainder` on every change; save is blocked unless it is zero. Transfers use the same picker (`[Account]` entries; investment accounts excluded). "Go to other side": opens the other account, filters to that date, and selects the entry.
- **11 Edit, void, delete, clear**: double-click or Enter edits in place. Context menu (`ContextMenu.svelte`): Edit, Mark cleared/unmarked, Go to other side, History, Void, Delete. Void and delete ask first, then `withConfirmation` handles a `confirmation_required` from Rust (reconciled entries).
- **12 Audit view** (`HistoryModal.svelte`): per-field changes from `audit_history`, newest first.
- **13 Integrity check** (`IntegrityModal.svelte`): runs on open, "Run again", issue table.
- **14 Tests**: `draft.test.ts` (build, split signing, QuickFill, round trip of a split), `keys.test.ts`, `EntryEditor.test.ts` (date keys, Enter saves, Esc, D-10, QuickFill, split remainder blocks save), `RegisterGrid.test.ts` (columns, today line, sort, arrow/space/Enter). IPC mocked. `vite.config.ts` gained `svelteTesting()`.
- **15 Payee editor** (`PayeeManager.svelte`): rename, memorized defaults, hide, merge, delete (in use gives a message from Rust). Payees are created only by entering a transaction.
- **16 Category and tag management** (`CategoryManager.svelte`, `TagManager.svelte`): create, rename, re-parent, flags, hide, merge, delete. Built-in categories are shown but locked. All three screens are tabs in `views/Manage.svelte`, reached from "Payees, categories, tags".

### Decisions (items 5–16)

- An entry needs a category, a transfer account, or a split. Uncategorized entries are not allowed: the engine has no uncategorized posting, and a non-zero entry with no lines does not balance.
- Tag column and tag picker use the transaction's first tag. The entry row edits one tag per transaction; extra tags on lines are preserved on edit but not editable.
- The register does not search the payee list on every keystroke for QuickFill lookup; it uses the last `payee_search` result, then the full payee list loaded at start.
- Filters reset when switching accounts.
- "Go to other side" leaves a one-day date filter on; Clear all removes it.
- Category kind cannot change after creation in the UI (only income and expense are offered; equity is system-only).
- Clr shows `c` or `R`; the Clr column click on a row is Space or the menu, not a mouse click on the cell.

### Bug found on first real run

Selecting an account did nothing: `App.svelte` rendered the view with `{@const View = views[viewState.current]}`, which did not swap the component when the view changed (state changed, DOM did not). Now `const View = $derived(views[viewState.current])`. `App.smoke.test.ts` renders the whole app with IPC mocked and covers select-account and view switching, so this class of bug is caught. Lesson: jsdom component tests missed it because no test mounted `App`.

### Known gaps from 3b

- **Not exercised in the real app.** All checks are Vitest (jsdom, IPC mocked), `svelte-check`, and `vite build`. Layout, focus behavior in a real webview, and NFR-040 in the UI are unconfirmed.
- Paging, not virtualization. Sorting or filtering resets to page 0.
- Account reordering (ACCT-240) is by the Sort order number in the account modal; no drag and drop.
- No UI toggle for `showClosedAccounts`.
- Settings (page size, sidebar/dropdown, theme) are not persisted (SET-070).
- Balances on credit and liability accounts show as stored (negative = owed).
- Split lines keep their tags and cleared status on edit but the split panel does not edit them. No per-line tag picker.
- Payee suggestions and the payee filter list load all payees; fine at hundreds, revisit if it grows.
- No keyboard shortcut list or focus trap in modals beyond Esc and initial focus.
- Accessibility pass and the icon bar (UI-020) are not done.

### 17 Docs and close-out

This file. The spec is unchanged: no requirement or convention changed. The spec header still says 0.3.4.

### Exit criteria (spec §24, Phase 3)

- Stan enters a month of transactions by keyboard. (**To confirm by hand**; the keyboard paths are unit-tested.)
- The generator loads a multi-year dataset (done in 3a).
- The register meets NFR-040 in the running app (measured in 3a at the query level; **confirm in the UI**: `just dev`, Load sample data, open the largest account).

### Decided with Stan

- **D-10:** separate Payment and Deposit columns. The UI splits the signed amount for display (`src/lib/format/`). The entry row has both fields; typing in one clears the other. Recorded in spec 0.3.3.
- **Today line** (REG-070): a horizontal line across the full register row between the last entry dated today or earlier and the first future-dated one. Future rows are dimmed. Stan had no preference; full-width chosen.
- **Category, tag, and payee screens:** the spec's phase table (§24) gives them no phase. Stan wants all three in 3b. Payee editing (memorized defaults, rename, hide, merge), category management (create, rename, re-parent, merge, hide), and tag management (create, rename, merge, hide) are required 3b items 15 and 16.
