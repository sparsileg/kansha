# Phase 7 — Reports and dashboard

Spec: 0.3.13. Engine, IPC, and UI in one pass. `just check` green.
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
printing, the dashboard. Not built (Stan, 2026-09-27): tithing,
holdings, allocation, cash flow, transaction report, account balances
(RPT-120, 130, 160, 170, 180, 190, 200).

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
  PDF by printing to PDF from the print dialog.
- Drill-down: a transaction row opens its register on that date; a Net
  Worth account opens its register up to the column date; an
  Income/Expense category opens Itemized Categories for that category
  (and subcategories) and period.
- Dashboard (DSH-010…030): net worth and parts, this month's income
  and spending, 12-month net worth line, overdue plus next 14 days of
  scheduled items, warnings (missing or stale prices, uncleared
  transactions over 60 days old, integrity problems).
- Menu: flat Reports menu with "Investing:", "Spending:", "Tax:"
  prefixes (no submenus yet); Capital Gains appears twice.

## Known gaps

- Not checked against the Quicken samples by Stan; column widths,
  fonts, and print layout untested on paper.
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
