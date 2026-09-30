# Prototype review (spec 0.4)

Date: 2026-09-29. Spec 0.3.37 → 0.4. App 0.7.0. Covers Phases 1–8.

Method: every [1.0] requirement ID in the spec checked against the
code, tests, and phase notes. Not a bug review of the code's logic;
that is the next pass (all phases, in order of risk: 1–2, 6, 4, 7, 8,
3, 5). MIG is Phase 9 and not audited.

Result: most [1.0] requirements are built and tested. Below: what the
audit found, what Stan decided, and what was done.

## 1. Findings and decisions

| Finding | Decision (Stan) | Done |
|---|---|---|
| BAK-030: spec said Settings warns about a backup folder on this computer; the note was removed in 0.3.33 | Drop "and Settings" | spec |
| Timed backups had no requirement of their own (under SET-050) | New requirement | BAK-045 (spec; already built in 0.3.37) |
| Start screen said backups are made "on closing and when you ask" | fix | text names timed backups |
| ACCT-100: "Show in icon bar" checkbox did nothing | Remove it | checkbox gone; column unused |
| Account dialog scrolled sideways (long tax-line choices widened it) | fix | controls shrink to the dialog |
| PRC-050: Holdings did not flag stale prices | fix | ⚠ column and a note under the report |
| AUD-020: no account history in the UI | fix | History… in the account dialog |
| REG-080: context menu had no Split | Add Split | Split opens the edit with its split lines |
| Search hit in an investment account did not select the transaction | fix | selected and scrolled into view |
| UI-060 undo not built | Build: last register change, one level | Edit > Undo, Ctrl+Z |
| SET-040 book-wide lot method not built | Build: default for new accounts | Settings; account dialog takes it |
| PRC-030 / D-40 | PRC-030 = manual price list import (ticker, price, optional `MM/DD/YYYY`; date picker; file picker or drop); replaces the dated CSV import; QIF prices only through MIG-140. D-40: Yahoo first behind an interface, keyed provider later, latest price only | both built |
| Other missing [1.0] items | Keep their status for now | listed in §4 |
| Spec out of date (15 items) | Make the spec accurate | spec 0.4 |
| Stale phase notes | Make them accurate | notes updated |

The Investments screen's stale tooltips said "more than a week old";
the threshold has been a setting since Phase 8, so they now say "out
of date (older than the stale-price limit)".

## 2. Built (spec 0.4)

**⚠ API change** (`just bindings` run): `price_import_preview` and
`price_import` take a `date`; `account_defaults` reads the book and
returns a result; `Settings` gains `default_lot_method` and
`price_download`; new commands `undo_status`, `undo_apply`,
`prices_download`. **No schema change.**

New dependency: `ureq =2.12.1` in `src-tauri` (rustls; all its crates
build on MSRV 1.85). `serde_json` gains the `raw_value` feature.
Tauri window `dragDropEnabled: false`, so a file dropped on the price
dialog reaches the page.

### Files

- `crates/kansha-core/src/undo.rs`: `before`, `after`, `available`,
  `apply`; `Undo`, `Before`.
- `crates/kansha-core/src/persistence/undo.rs`: row snapshots of one
  transaction (`read`, `restore`, `is_scheduled`, `audit_mark`).
- `crates/kansha-core/src/securities/import.rs`: rewritten for the
  price list format.
- `crates/kansha-core/src/securities/download.rs`: `Provider` (Yahoo),
  `targets`, `Provider::url`/`parse`, `store`.
- `crates/kansha-core/src/persistence/securities.rs`:
  `find_by_ticker`.
- `crates/kansha-core/src/accounts/mod.rs`: `defaults` (SET-040).
- `crates/kansha-core/src/settings/mod.rs`: `default_lot_method`,
  `price_download`.
- `crates/kansha-core/src/reports/investing.rs`: Holdings stale column
  and note.
- `src-tauri/src/state.rs`: undo slot, `write_undoable`, `undo_label`,
  `undo`, `forget_undo` (book opened, restored, or closed).
- `src-tauri/src/commands/ledger.rs`, `invest.rs`: register commands
  record their undo; `undo_status`, `undo_apply`; `prices_download`
  (fetches off the main thread, 15 s per request).
- UI: `components/invest/PriceImportModal.svelte` (new);
  `CsvImportModal.svelte` seeds lots only; `HistoryModal.svelte` takes
  an entity; `AccountModal.svelte` (History…, no icon-bar checkbox,
  overflow fix); `RegisterGrid.svelte` / `EntryEditor.svelte` (Split
  in the menu); `InvestmentAccount.svelte` (selection, reveal);
  `SettingsModal.svelte` (lot method, price download);
  `shell/actions.ts` (`undoLast`, `downloadPrices`);
  `shell/menus.ts` (Edit > Undo, Tools > Import Prices…, Download
  Prices); `ui/undoKey.ts` (Ctrl+Z); `invest/rows.ts` (tooltips).

### Decisions made in the build

- **Undo is a row snapshot, not replayed commands.** Before and after
  a change, every row the transaction owns is read; undo writes the
  "before" rows back with their IDs. That covers void and delete (no
  un-void or re-create path exists in the engine) and keeps IDs,
  reconciliation links, and lots exact. The header is updated in
  place so a schedule occurrence's link holds.
- **Undo is refused** unless the newest audit entry is still the
  change's own and the rows are still as it left them.
- **Undo's audit entries use origin `ui`.** I had proposed an `undo`
  origin; the `audit_log` CHECK allows only `ui`, `import`,
  `scheduler`, `system`, and changing it means rebuilding the
  append-only table. The entries are ordinary `create`, `update`, or
  `delete` with before and after. A migration can add the origin
  later (it could go with dropping the tithing columns).
- **Deleting a scheduled transaction offers no undo** (it changes the
  schedule, REC-160). Entering from a schedule is not a register
  change and offers none either.
- A payee created or memorized with a new transaction stays after the
  transaction is undone.
- **Price list import:** fields split on commas if the line has any,
  else on whitespace; the price may carry `$` but no thousands commas;
  the date is `MM/DD/YYYY` only (one-digit month and day allowed);
  tickers match tickers only (not names), ignoring case; a price must
  be above zero; paste is gone (file or drop).
- **Download:** latest price per security (`regularMarketPrice`), dated
  by the market time in the exchange's zone, rounded half-even to 6
  decimals from the reply's text. Requests carry a `Kansha/<version>`
  user agent (Yahoo answers 429 to an empty one). A failed ticker is
  listed in the status bar, not stored. No backup before a download
  (it only adds prices; the timed backup follows).
- **SET-040** is only the starting value for new investment accounts;
  the lot method order (sale, security, account) is unchanged.

### Tests

Engine: `tests/integration/undo.rs` (8: create, edit with split and
IDs, void, delete, cleared, refused after another change, reconciled
asks, investment sale and deleted buy restore lots, scheduled delete
offers none); `repositories.rs` (SET-040 default, download setting);
`invest.rs` (download targets and store); `performance.rs` (Holdings
stale column and note); unit tests in `securities/import.rs` and
`securities/download.rs` (real Yahoo reply, errors, half-even
rounding, market day across midnight UTC, URL encoding); scenario
`INV-011` rewritten for the price list format. Frontend:
`PriceImportModal.test.ts`, `AccountModal.test.ts`,
`shell/actions.test.ts`, `ui/undoKey.test.ts`, and new cases in
`RegisterGrid.test.ts` (Split), `InvestmentAccount.test.ts`
(selection), `SettingsModal.test.ts`, `menus.test.ts`. The Split test
found a bug before it shipped (the flag was cleared before it was
read).

Not tested: the HTTP call itself (`fetch_all`; the endpoint was
checked by hand with curl, 200 with Kansha's user agent); drag and
drop in the real window; the account dialog's width in WebKitGTK; undo
and download in the running app.

## 3. Spec changes (0.4)

Header status; ACCT-100; REG-080; INV-010 table restored (the filling
had run it into prose); POS-030's old [Later] recommendation removed;
PRC-030, PRC-040 ([1.0]), PRC-050; DSH-030 as built; AUD-020;
BAK-030; BAK-045 new; SECU-070; UI-010, UI-020 as built; UI-060;
SET-040, SET-050, SET-070 (font); §16.1 CI off; §16.3 R4 (`ureq`,
`raw_value`), R5; §17.2 modules, §17.3, §17.4 layout; §18 void and
undo, stale threshold, undo and price rules; §20.2; §23 rewritten as
what the prototype built; §24 Phase 6; §25 now points to CLAUDE.md;
D-40 decided; Appendix A in one order, newest first.

## 4. Still missing (status kept, Stan 2026-09-29)

| ID | What |
|---|---|
| UI-030 | Account view tabs (Register, Scheduled, Reconcile history) |
| UI-050 | Global keyboard shortcuts (Ctrl+N in the register and Ctrl+Z only) |
| RPT-040 | Compare with a prior period |
| RPT-120, RPT-190, RPT-200 | Account balances, cash flow, transaction reports (D-90) |
| TAG-030 | Group a report by tag (filter works) |
| TEST-070 | `just trace`; RPT, DSH, UI tests cite no IDs |
| TEST-090 | `insta` (text snapshots in `reports.rs` stand in) |
| TEST-150 | Coverage |
| TEST-140 | CI, disabled since 2026-09-24 |

Also open by the phase exit rules: `just test` on Windows (Phases 5,
7, 8); Stan's report review against the Quicken samples (Phase 7);
hands-on restore, timed backup, folder picker, window geometry
(Phase 8); and now undo, price import, and price download.

## 5. Open decisions

| ID | Status |
|---|---|
| D-70 | 6-decimal shares: settle with the brokerage CSVs (Phase 9) |
| D-80 | Parallel run 2–3 months: to confirm |
| D-90 | 1.0 report list: settles RPT-120/190/200 |
| D-130 | Synthetic data only: ends when Phase 9 imports real data |

Placeholders P-01–P-05 belong to Phase 9.

## 6. Known items carried from earlier phases

- Shift+F10 and the Menu key do not open the register context menu;
  the right-click menu is placed wrong near the window's bottom
  (Phase 3).
- Register at a 24 px base font: columns overflow; the "Today" label
  sits over a balance; native checkboxes do not scale (shell).
- Investment register: no keyboard grid; not measured at thousands of
  rows (NFR-040); a transfer-in row opens only from the sending side.
- No lot replay: fixing an old trade means re-entering later ones.
- Save PDF is Linux only.
- Dockable registers: three questions open (`shell.md`).
- Engine error messages show ISO dates, not the user's format.

## Large book (NFR-040, NFR-050)

`just perf` (release, 2026-09-30, Linux): 108,662 transactions over 12
accounts and 20 years; busiest register Checking, 19,042 rows. Book
built in 146 s.

| Measure | Time | Limit |
|---|---|---|
| Register, all rows | 247 ms | 1 s |
| Register, first page | 206 ms | 1 s |
| Register, text filter | 248 ms | 1 s |
| Net Worth, 20 years by month | **11.2 s** | 2 s |
| Net Worth, one year by month | 820 ms | 2 s |
| Income/Expense by category, one year | 33 ms | 2 s |
| Itemized Categories, one year | 36 ms | 2 s |
| Dashboard | **2.6 s** | 2 s |
| Integrity check | 1.7 s | none |

Misses: Net Worth over many months (cost grows with months times
postings) and the dashboard (likely its net worth graph). Not fixed.
The first register page costs nearly as much as all rows: the window
function scans the whole account (phase-3 notes).

## Stan's findings

(to add)
