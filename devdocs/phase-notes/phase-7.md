# Phase 7 — Reports and dashboard

Spec: 0.3.16 (0.3.13 first pass, 0.3.14 toolbar, 0.3.15 report
windows). Engine, IPC, and UI in one pass. `just check` green.
Built from `devdocs/reports.md`, Stan's Quicken samples in
`report-samples/`, and the scope agreed in chat.

**Exit criteria (spec §24):** report snapshot tests pass (text
snapshots in `tests/integration/reports.rs`); every detail row has a
drill-down (`every_figure_drills_to_a_transaction_or_account`).
**Open:** Stan's hands-on review against the Quicken samples; `just
test` on Windows.

**⚠ Schema change:** migration 0003 `tax_line` (35 built-in lines),
`category.tax_line_id`, `account.tax_line_out_id`, `account.tax_line_in_id`;
built-in Interest, Dividends, and capital gain distribution categories
mapped. `saved_report` was already in 0001.
**⚠ API change:** `CategoryFields.tax_line`, `AccountFields.tax_line_out`
and `tax_line_in`; 11 new commands (below); `just bindings` run.

## Scope

Built: Capital Gains (Investing and Tax menus), Net Worth, Itemized
Categories, Itemized Payees, Income/Expense by Category, Tax Schedule,
Tax Summary, saved reports, Customize dialog, drill-down, CSV export,
printing, the dashboard. Not built (Stan, 2026-09-27): tithing, cash
flow, transaction report, account balances (RPT-120, 130, 190, 200).
Investment Performance, Investment Income, Holdings, and Asset
Allocation (RPT-160, 170, 180, 310) came later with the Phase 6
follow-up; see `phase-6.md`.

## Files

- Migration `0003_tax_lines.sql`.
- `categories`: `TaxLineId`, `TaxLine`; `CategoryFields.tax_line`.
- `accounts`: `AccountFields.tax_line_out`, `tax_line_in`.
- `persistence/reports.rs`: tax lines, posting facts (one query for a
  range), first transaction date, old uncleared postings, saved report
  CRUD (audited, names unique ignoring case).
- `reports/mod.rs`: `ReportKind`, `DatePreset`, `Interval`, `Subtotal`,
  `DetailSort`, `DateRange`, `ReportSettings` (+ `defaults`),
  `SavedReport`, output `Report`/`Column`/`Row`/`Drill`, `run`,
  `columns`.
- `reports/range.rs` presets and periods; `tree.rs` totals, hidden
  columns, whole-dollar rounding; `facts.rs` transactions → report
  lines; `itemized.rs` (categories, payees, tax summary);
  `income_expense.rs`; `capital_gains.rs`; `net_worth.rs`; `tax.rs`
  (Tax Schedule); `chart.rs` (axis and positions); `csv.rs`;
  `dashboard.rs`.
- `sample.rs`: salary, charity, and property tax categories get tax
  lines.
- `src-tauri/src/commands/reports.rs`: `report_defaults`,
  `report_columns`, `report_range`, `report_run`, `report_export_csv`,
  `saved_report_list`, `saved_report_create`, `saved_report_update`,
  `saved_report_delete`, `tax_line_list`, `dashboard`.
- UI: `views/Reports.svelte` (toolbar: date range, Customize, Save,
  Save As, Expand/Collapse All, Export CSV, Print; Saved Reports is
  in the Reports menu),
  `components/reports/ReportTable.svelte`, `ReportChart.svelte`,
  `CustomizeReportModal.svelte`, `SavedReportsModal.svelte`;
  `state/reports.svelte.ts`; `reports/meta.ts` (per-report options,
  labels, menu map), `reports/rows.ts` (tree → lines);
  `format/report.ts`; `views/Dashboard.svelte` rewritten. Reports menu
  items live; Tax line pickers in Tools > Categories and the account
  dialog. Chart colors `--chart-*` and print CSS in `App.svelte`.
- Tests: `tests/integration/reports.rs` (every report on one book as
  text snapshots, filters, totals only, hidden columns, whole dollars,
  voids, tax-deferred exclusion, drill-down, saved reports and audit,
  old settings JSON, CSV, dashboard, all reports on the sample book),
  migration 0003 test, unit tests in `range`, `tree`, `chart`, `csv`.
  Frontend: `reports/reports.test.ts`, `views/Reports.test.ts`,
  `views/Dashboard.test.ts`.

## Decisions

- One output shape for every report: columns plus a row tree; groups
  carry totals; an expanded group closes with "Total <name>" (the UI
  adds that line). Table, CSV, and print share it.
- Report sign: income and incoming money positive, spending negative,
  as Quicken prints. Liabilities in Net Worth show as amounts owed.
- Transfers appear once from each included account's side, so with
  every account included the TRANSFERS section totals zero (as in the
  Itemized Categories sample).
- Splits: one row per split line (Quicken's payee report shows
  "--Split--"; the category column here names each line's category).
- Voids are left out (Quicken lists them at 0.00 as **VOID**).
- Opening balances (equity) are not income or expense and are left out.
- Tax reports use taxable accounts only (IRA income stays out); an IRA
  distribution counts through the IRA's "transfers out" tax line, even
  into a tax-exempt account (a Roth conversion).
- Realized gains: Tax Summary lists the Realized Gain/Loss category;
  Tax Schedule builds Schedule D from the lots (short, long, no
  holding period) and ignores the category, so nothing counts twice.
- Capital Gains: default accounts are taxable investment accounts
  (Quicken's default); an explicit list overrides.
- Filters: `None` = all (new accounts and categories are included
  later); Select All sets `None` again.
- Graphs: hand-drawn SVG (D-140). Rust picks a round axis and places
  values on 0–10000; the frontend scales only. Blue solid bars, orange
  hatched bars, and a line with square marks.
- CSV goes to Downloads (`<title> <date>.csv`, never overwriting);
  PDF likewise through Save PDF… (see "Save PDF" below).
- Drill-down: a transaction row opens its register on that date; a Net
  Worth account opens its register up to the column date; an
  Income/Expense category opens Itemized Categories for that category
  (and subcategories) and period.
- Dashboard (DSH-010…030): net worth and parts, this month's income
  and spending, 12-month net worth line, overdue plus next 14 days of
  scheduled items, warnings (missing or stale prices, uncleared
  transactions over 60 days old, integrity problems).
- Menu: Reports menu is Saved Reports…, then Investing, Net Worth,
  Spending, and Tax submenus (see "Report windows" below); Capital
  Gains appears under Investing and Tax.

## Toolbar follow-up (spec 0.3.14)

Stan's six items, 2026-09-27:

- Itemized Categories and Payees: "Sort by:" dropdown (Date/Account,
  Account/Date, Amount). `DetailSort::Date` now breaks date ties by
  account order.
- Same reports: Date, Account, and Num headings sort; a second click
  reverses. New `DetailSort::Num` (numbers numerically, then text, then
  blank) and `ReportSettings.sort_desc`. Reversing flips the chosen
  key only; ties stay in date and entry order.
- Capital Gains: "Subtotal by:" dropdown; "Don't subtotal" last.
- Income/Expense by Category and by Payee: "Interval:" dropdown.
- New report Income/Expense by Payee (`ReportKind::IncomeExpensePayee`):
  INCOME and EXPENSES, one row per payee (names ignoring case, no payee
  last), no transfers. `Drill::Payee` opens Itemized Payees for that
  payee and period; "(No payee)" opens it with the report's payee filter.
  Menu: "Spending: Income/Expense by Payee".
- Date Range dropdown always lists "Custom dates…": opens a From/To
  dialog; "Change Dates…" reopens it while a custom range is shown.

**⚠ API change:** `ReportKind` `income_expense_payee`, `DetailSort`
`num`, `ReportSettings.sort_desc`, `Drill` `payee`; `just bindings` run.
No schema change (settings JSON gains a defaulted field).

## Report windows and page (spec 0.3.15)

Stan's deferred items, built 2026-09-28. No Rust, schema, or API change.

- **Windows and dock (UI-040).** `state/windows.svelte.ts` is generic:
  a window has an id, a kind, and a live label. Showing one is the
  `"window"` view (`params.window`), so Back/Forward include it and
  going to any other view leaves it in the dock. `viewState.base` is
  the view under the windows (Minimize goes there); `viewState.forget`
  drops a closed window's history entries. Kinds register close hooks.
  `components/shell/Dock.svelte` (text labels, repeated names numbered,
  × closes, clicking the top one minimizes) and `WindowFrame.svelte`
  (name, Minimize, Close). App maps kind `report` to
  `components/reports/ReportWindow.svelte` (was `views/Reports.svelte`;
  the "reports" view and its "Choose a report" list are gone).
- `state/reports.svelte.ts`: `ReportInstance` per window; `reportState`
  opens windows and holds `savedOpen` (the Saved Reports dialog is now
  in App). A category or payee drill-down opens a second window.
- Showing a window again reruns its report (each show remounts it).
- **Save prompt (RPT-020).** Closing a report whose settings differ
  from how it opened or was last saved asks Save / Don't Save / Cancel.
  Save updates a saved report; a new one shows its Save dialog, then
  closes. `confirmState.choose` gives the dialog named buttons.
- **Submenus.** `MenuItem.items`; `MenuBar` opens a submenu on hover,
  click, Enter, or Right; Left or Esc returns. Item ids unchanged, so
  navigation bars keep working; `leafItems` feeds the nav catalog.
- **Hide Graph / Hide Report** buttons (icon plus text) at the upper
  right of the graph and the table, with a rule between; view state
  only, not saved, not printed.
- **Frozen heading row**: `position: sticky` in the page's scroll area;
  title and graph scroll away. Print repeats it per page.
- **White page**: the report area uses white paper, dark text, and the
  light theme's graph colors in every theme; toolbar keeps the theme.

## Panels, Exit, calendar day dialog (spec 0.3.16)

Stan's requests, 2026-09-28. No Rust, schema, or API change.

- Dock labels use the browser's button text size, like the navigation
  bar.
- Calendar, Reminders, Accounts (Tools > Accounts), and Reconcile are
  single windows (`shell/panels.ts`, `windowState.openSingle`); their
  view ids are gone from `ViewId`. Home = Calendar or Reminders opens
  that window.
- File > Exit: `windowState.mayQuit` runs each window's close check
  without closing anything; a changed report asks to save; Cancel
  stops the exit. Saving a never-saved report shows its Save dialog and
  the exit stops; choose Exit again after saving.
- `components/DayModal.svelte` replaces `OccurrenceModal` in the
  calendar. It lists the day's occurrences with done ones included
  (respects the account filter). Enter and Skip only for the schedule's
  next open one (disabled buttons say why); Edit opens an entered
  transaction in its register, otherwise the schedule; double-click or
  Enter on a row does Enter when possible, else Edit.

## Follow-ups (2026-09-28, spec 0.3.17)

- Day dialog buttons: New Schedule on the left; Enter, Edit, Skip,
  Close on the right.
- The title bar's close box asks to save changed reports, as File >
  Exit does (`guardWindowClose` in `shell/nav.ts`, Tauri
  `onCloseRequested`). Capability `core:window:allow-destroy` added:
  Tauri closes the window through it once the handler allows.
- A window's name in its frame is its heading (`h1`); Calendar,
  Reminders, Accounts, and Reconcile no longer have their own.

## Printing (2026-09-28, spec 0.3.19)

- Printing leaves the graph (and the rule under it) out; the report is
  9 pt with a 12 pt title, whatever the screen font size.
- Checked by printing the app's own markup and built CSS to PDF through
  WebKitGTK 4.1 (the Linux web view): a 14-column Net Worth fits one
  landscape page. Stan's blank landscape pages did not reproduce
  (portrait, landscape, mismatched dialog settings, scrolled report
  all print).
- Stan: every report prints blank pages in landscape, light and dark,
  Print to File (PDF) from the GTK print dialog (which has no preview).
  Only the dialog path is untested here, so the cause is WebKitGTK's
  dialog landscape handling or how Tauri starts printing. WebKitGTK
  ignores CSS `@page { size: landscape }`. Resolved in 0.3.20; see
  "Save PDF" below.

## Save PDF (2026-09-28, spec 0.3.20)

- Cause found: a standalone WebKitGTK window (no Tauri) with the real
  report page printed one blank page in landscape through the GTK
  print dialog, from `window.print()` and from WebKit's own
  `run_dialog` alike. The same page prints fine when the orientation
  is set in code.
- The toolbar's Print… is now **Save PDF…**: Portrait or Landscape
  (remembered per open report), then the PDF goes to Downloads
  (`<heading> <date>.pdf`, never overwriting), opens in the PDF viewer
  (preview; print to paper from there), and "Saved to …" shows.
- `src-tauri/src/commands/pdf.rs`: `report_save_pdf` gets the
  `webkit2gtk::WebView` through `with_webview`, prints to GTK's "Print
  to File" printer with the page setup set in code, and waits for
  `finished`/`failed`. `download_path` and `internal` moved into
  `commands/reports.rs` helpers shared with CSV export.
- Tried and dropped: a Print… button with the GTK dialog preset to
  landscape still printed blank pages (Stan, 2026-09-28).
- New Linux-only deps in `src-tauri`: `gtk` 0.18, `webkit2gtk` 2.0 (the
  versions wry already uses). **⚠ API change:** `report_save_pdf`,
  `PageOrientation`; `just bindings` run.

## Known gaps

- Not checked against the Quicken samples by Stan; column widths,
  fonts, and print layout untested on paper.
- Save PDF is Linux only; Windows and macOS show an error. The file
  printer name "Print to File" may differ on a non-English GTK.
- Security Types and Investing Goals tabs (Quicken) not built: Kansha
  has no such concepts.
- Organization option (Quicken's Itemized Categories) not built; only
  Income & Expense.
- Report date comparison to a prior period (RPT-040) not built.
- Wide reports (Net Worth by month) print as one wide table; Quicken
  splits columns across pages.
- Last backup age warning (DSH-030) waits for backups (Phase 8).
- Upcoming days (DSH-020) fixed at 14 until settings (SET).
- The DAF gift of 7/15/2025 will show as a 0-gain sale (charitable
  gift disposal still deferred).
- Back/Forward history does not restore an earlier report's settings.
- A window's scroll position resets each time it is shown (collapsed
  groups and hidden parts are kept).
- Windows fill the view area; no moving or resizing.
- The account list sidebar (AccountPanel) is not a window; "Accounts"
  in the dock is Tools > Accounts.
