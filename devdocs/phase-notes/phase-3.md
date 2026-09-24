# Phase 3 — Register UI

Spec: 0.3.4. Split into 3a (core queries, sample data, IPC) and 3b (Svelte UI), agreed with Stan.

## Status

| Sub-phase | State |
|---|---|
| 3a Core queries, sample data, IPC | Done. `just check` green. |
| 3b Svelte UI | Built. `just check` green (114 frontend tests). Hands-on tests 1-8 done by Stan; open issues listed under "Phase 3 close-out summary". |

3a is committed. All of 3b is uncommitted in the working tree; Stan commits in GitKraken.

## Phase 3 close-out summary

**Files created (3b, beyond 3a):** `src/lib/api/`, `src/lib/format/`, `src/lib/state/*.svelte.ts`, `src/lib/register/{draft,keys,match}.ts` (+ tests), components `AccountModal`, `AccountSelector`, `AccountBalance`, `CategoryManager`, `ConfirmDialog`, `ContextMenu`, `EntryEditor`, `FilterBar`, `HistoryModal`, `IntegrityModal`, `Modal`, `PayeeManager`, `RegisterGrid`, `TagManager`, `TargetCombo`; views `Account`, `Dashboard`, `EmptyBook`, `Manage`; `App.smoke.test.ts`. `TargetSelect.svelte` was replaced by `TargetCombo.svelte` (type-ahead).

**Decisions:** D-10 separate Payment/Deposit columns; Today line full-width; payee/category/tag screens built in 3b; type-ahead Category with accounts; Enter moves to next row; amount fields numeric-only; Tithable and Giving removed from the category form and list (columns and API stay, values preserved on edit; reports will cover them); Tax-related stays. Details in the sections below.

**⚠ API change:** 3b added the commands listed under "3b — what was built". `just bindings` was run. **No schema change** in Phase 3b.

**Known gaps and open issues:**
- Shift+F10 and the Menu key do not open the row context menu in the running app. A native `contextmenu` fallback did not fix it. Cause unknown.
- Right-click menu placement is still wrong near the bottom of the window despite viewport clamping in `ContextMenu.svelte`.
- No UI toggle for `showClosedAccounts`, so closed accounts cannot be seen or reopened. Phase 4.
- Paging, not continuous scroll. After Phase 4.
- Account panel is a sidebar/dropdown mode switch. Redesign in Phase 4.
- Settings not persisted (SET-070); other gaps under "Known gaps from 3b".

**Hands-on tests:** all 8 confirmed by Stan except the open issues above.

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
- Written before the first real run; see "Hands-on testing" for what has since been confirmed.

### Decisions

- Opening an account resets filters and sorts by date ascending, on the last page, scrolled to the bottom so the newest entries sit next to the entry row (Stan's choice, replacing the earlier newest-first plan). A new sort column also starts ascending. A filter, sort, or clear-all resets to page 0.
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
- **6 Register grid** (`RegisterGrid.svelte`): Date, Num, Payee, Payment, Deposit, Category, Tag, Memo, Clr, Balance. Paging (100 rows, "Previous/Next"), not virtualized. Opens date-ascending on the last page, scrolled to the bottom. Sort by clicking a header (Payment and Deposit both sort by amount). Split rows show `--Split--` from the query. Today line (REG-070) where `future` flips between adjacent rows, drawn only on date sort. Future rows dimmed. Void rows struck through. Footer: current, cleared, ending, available credit (REG-060), entry count, paging.
- **7 Filters** (`FilterBar.svelte`): date range, payee, category, tag, cleared, text (250 ms debounce), Clear all.
- **8 Keyboard entry** (`EntryEditor.svelte`, `register/draft.ts`, `register/keys.ts`): the entry row is pinned below the grid. Tab order is Date, Num, Payee, Payment, Deposit, Category, Tag, Memo, Enter. Enter saves; Esc cancels. In Date: `+` or `=` next day, `-` previous day, `t` today. After a save the row resets, keeps the date, and refocuses Date. Grid keys: arrows, Home, End, PageUp, PageDown move; Enter edits; Delete deletes; Space toggles cleared; Insert or Ctrl+N focuses the entry row; Shift+F10 or the menu key opens the context menu.
- **9 QuickFill**: payee suggestions from `payee_search` through a datalist. On change, an exact name match fills empty category, tag, memo, and amount from the payee's defaults. Never overwrites typed values. Edits do not QuickFill. `payee_name` is always sent; Rust finds or creates the payee.
- **10 Splits and transfers**: choosing `--Split--` opens the split panel (two blank lines). Line amounts are typed as magnitudes and signed like the total (payment or deposit). The remainder comes from `split_remainder` on every change; save is blocked unless it is zero. Transfers use the same type-ahead (`[Account]` entries; investment accounts excluded). "Go to other side": opens the other account, filters to that date, and selects the entry.
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

### Fixes from hands-on use

- **Edit did not close or refresh.** `entry_update` succeeds with `null` data, and `withConfirmation` returned `null` for "user declined", so the editor treated success as a decline and stopped. `withConfirmation` now returns the `DECLINED` symbol. Tests cover save and the confirm-and-repeat path for an existing transaction.
- **Enter moves to the next row.** Enter in an in-place edit saves and selects the next row (the new-entry row after the last one); the grid keeps focus, so Enter again edits that row. If the edit is untouched, nothing is written (no audit entry) and the selection still moves on. Esc cancels and keeps the row selected.
- **Amount fields are numeric-only.** Payment, Deposit, and split amounts accept digits, commas, and one decimal point (max two decimals); other characters are blocked as typed and stripped on paste (`sanitizeAmountInput`). No sign: the column carries the direction.
- **Split panel.** Column headings (Category or transfer account, Amount, Memo) and a one-line explanation. Choosing `--Split--` puts the whole amount on line 1 and moves focus there. Each empty amount is offered what is still unassigned when its line gets focus (from `split_remainder`; nothing offered when the split is complete or over-allocated). Tab out of the last line with an amount left opens a new line. The remove button is out of the Tab order.
- **Layout fits the window.** The app is exactly one window tall and the register's row area takes whatever the header, entry row, and footer leave (it shrinks; it no longer has a fixed 60 vh cap). A growing entry row or split panel therefore pushes nothing off the bottom.
- **Split panel placement.** The panel is above the entry line when the register sorts ascending and below it when descending. The new-entry row stays pinned at the bottom in both.
- **Saved row never clipped.** After a save the grid pins that row fully in view and re-applies it whenever the rows area changes size, until the user scrolls or navigates. The entry row keeps a one-line message slot so "Saved..." or an error never changes its height (that resize was what left the last row partly hidden).
- **Payee completion.** Tab or Enter on a payee with exactly one visible prefix match takes it, runs QuickFill (new entries only), and moves on; Enter moves to Payment without saving.
- **Scrollbar over Balance.** The grid measures its scrollbar width and gives the header, entry row, and footer the same right edge, plus a 0.75 rem pad, so columns line up and Balance is clear of the scrollbar.

### Category type-ahead

The entry row's Category (and each split line's) is a type-ahead box, `TargetCombo.svelte`, not a dropdown. Typing filters categories and transfer accounts together; brackets are never typed (`register/match.ts`: every word must appear, case-insensitive; label-start matches rank first, then word-start, then anywhere). Up/Down move; Tab or Enter with the list open takes the highlighted match; Esc closes the list (a second Esc cancels the entry); with the list closed, Enter saves. The list opens only when the user types or presses Up/Down. The filter bar and manager screens still use plain dropdowns.

### Focus highlight

Focused inputs, selects, and textareas get a 3 px blue outline, a glow, and a yellow background (global rule in `App.svelte`), so the active field is obvious during Tab entry.

### Hands-on testing (Stan, `just dev`)

Legend: [x] confirmed by Stan in the running app; [ ] still to do.

- [x] Select an account; register and entry row appear (after the view-switch fix).
- [x] Enter a transaction; Enter saves and returns to the entry row; focus highlight; type-ahead Category with accounts.
- [x] Edit in place; Enter saves and moves to the next row (also when unchanged).
- [x] NFR-040: register meets the target with the sample data.
- [x] Split entry: panel placement (above when ascending), amounts offered, remainder.
- [x] Scrolling and layout: entry row and split panel fit the window; the saved row is never clipped.
- [x] 1. Transfer: enter by typing an account name; right-click, "Go to other side of transfer" (opens the other account, entry selected, one-day date filter on).
- [x] 2. Right-click menu (passed with the open issues below): Mark cleared, Void, Delete, History. Keys: Space, Delete, Shift+F10 on a selected row.
  - Open issue: Shift+F10 and the Menu key do not open the row menu in the running app. A native `contextmenu` fallback was added in `RegisterGrid.svelte`; it did not help. Cause not found. Look at again later.
  - Open issue: right-click menu is still not placed correctly (viewport clamping added in `ContextMenu.svelte`; Stan reports placement is still wrong). Look at again later.
- [x] 3. Filters: text, date range, category, tag, cleared; Clear all; empty result message "No entries match the filters."
- [x] 4. Account modal: create each account type; edit; Reveal account number; close an account with a balance (confirmation); reopen; delete an empty account; deleting one with transactions is refused.
  - Open issue (deferred to Phase 4): no UI toggles `settingsState.showClosedAccounts`, so a closed account cannot be seen or reopened. Add a "Show closed accounts" checkbox in the sidebar.
- [x] 5. Integrity check: no problems on the sample data.
- [x] 6. Payees, Categories, Tags screens: rename; merge (register updates); delete unused; delete used shows an error; built-in category locked.
  - Decision: Tithable and Giving checkboxes removed from the category form and list (reports will cover them). The `tithable` and `giving` columns and fields stay in the schema and API for now; editing a category preserves their values. Tax-related stays.
- [x] 7. Payee Tab/Enter completion on a unique match; Balance column clear of the scrollbar.
- [x] 8. Sidebar/dropdown toggle; dark and light themes; narrower window; Esc closes each modal.

Bugs found so far in hands-on use, all fixed and covered by tests (see "Bug found on first real run" and "Fixes from hands-on use"): view did not switch; edit save treated as declined; empty-row Enter error; Enter in a select; scrollbar over Balance; layout past the window; saved row clipped.

3b is committed by Stan in GitKraken. Phase 3 is done. The spec header stays 0.3.4 (no requirement or convention changed).

### Known gaps from 3b

- **Only partly exercised in the real app.** See "Hands-on testing". Automated checks are Vitest (jsdom, IPC mocked), `svelte-check`, and `vite build`; jsdom has no layout, so layout and focus fixes were confirmed only by hand.
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

- Stan enters a month of transactions by keyboard. Entry, in-place edit, and Enter-to-next-row confirmed by Stan; a longer real-data pass is optional.
- The generator loads a multi-year dataset (done in 3a).
- The register meets NFR-040 in the running app. **Confirmed by Stan.**

### Decided with Stan

- **D-10:** separate Payment and Deposit columns. The UI splits the signed amount for display (`src/lib/format/`). The entry row has both fields; typing in one clears the other. Recorded in spec 0.3.3.
- **Today line** (REG-070): a horizontal line across the full register row between the last entry dated today or earlier and the first future-dated one. Future rows are dimmed. Stan had no preference; full-width chosen.
- **Category, tag, and payee screens:** the spec's phase table (§24) gives them no phase. Stan wants all three in 3b. Payee editing (memorized defaults, rename, hide, merge), category management (create, rename, re-parent, merge, hide), and tag management (create, rename, merge, hide) are required 3b items 15 and 16.

### Deferred to Phase 4: account panel redesign

Replaces the `sidebar`/`dropdown` modes (`settingsState.accountNav`) and the top-bar mode button.

- One collapsible account panel with a toggle header labelled "Accounts" (never the current account's name).
- Open: header plus the full grouped account list at the side; register narrows.
- Closed: the panel disappears and the register expands to fill the space. Only the toggle and "Accounts" label remain, above the register at the panel's side.
- Clicking the header while closed shows the account list as a drop-down menu; picking an account selects it and closes the menu.
- Open/closed state persists between launches.
- Panel side (left or right) is a setting, default left; a settings screen can come later.
- Add "Show closed accounts" checkbox in the same panel (see test 4 issue).

### Deferred to after Phase 4: continuous-scroll register

Replaces pagination (`pageIndex`, `goToPage`, footer pager). Decided with Stan; do after the Phase 4 account-panel work.

- **Why:** paging loses running-balance context at page edges and slows scanning. Not a performance need: one rendered page already meets NFR-040 at 10,000 transactions.
- **Design:** one scroll area sized to `total` rows at a fixed row height. A sliding window of loaded rows; fetch the next or previous chunk near an edge, drop chunks far from the viewport.
- **Backend:** `register_query` already takes `limit`/`offset`, and balances are computed over the whole account before filter, sort, and paging, so they stay correct. ⚠ API change: add a command returning a row's offset for a `txn_id` (filtered and sorted), for jumps. Then `just bindings`. No schema change.
- **Anchoring:** open on the newest row (ascending) or the top (descending). Save, edit, and "Go to other side of transfer" scroll to a `txn_id`.
- **Frontend:** rework `RegisterGrid.svelte` and `register.svelte.ts`. Selection, PageUp/PageDown, Home/End work across unloaded rows (Home/End jump the scroll).
- **Check:** uniform row height (single-line columns); Today line; split panel and entry-row growth; saved-row pinning ("never clipped") against virtualization; scrollbar-width alignment.
- **Stopgap if postponed further:** larger page size plus a "Go to date" jump.
