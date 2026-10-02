# Tax reports: closer to Quicken — design and plan

Status: **proposed** (2026-10-02). Implements on top of spec 0.7.3;
spec goes to 0.7.4 when built.
Requirements: CAT-050, MIG-020, RPT-020, RPT-140, RPT-145.

## 1. Why

Stan compared Kansha's Tax Schedule and Tax Summary with Quicken
2013's for 2025 (screenshots in `P:\pCloudSync\kansha\`:
`quicken-tax-schedule.png`, `quicken-tax-summary.png`,
`kansha-tax-schedule.png`, `kansha-tax-summary.png`). Goal: make
Kansha's look and read like Quicken's, add what Quicken has and
Kansha lacks, remove nothing.

## 2. Findings

### 2.1 Same in both (keep)

- Tax Schedule: form → line groups with their transactions; no
  overall total.
- Tax Summary: INCOME / EXPENSES / TRANSFERS, category tree with
  subcategories, Tax Item column, OVERALL TOTAL.
- Both: Expand All / Collapse All, per-group toggles, Clr column,
  drill-down (RPT-030), investment action in Num, transfer tax lines
  (an IRA distribution on 1099-R).

### 2.2 Kansha has, Quicken does not (keep)

- Schedule D from lot disposals, short- and long-term (LOT-160).
- Investment-account dividends on Schedule B. Quicken's Tax Schedule
  shows only the 13.19 of `Div Income` in VBS-Cash; its own Tax Summary
  shows 16,291.51 of `_DivInc`. Kansha's 16,304.70 is what the
  1099-DIVs say.
- "(Form 8283 needed)" on the non-cash charity line.
- Customize filters, saved reports, CSV and PDF.

### 2.3 Quicken has, Kansha lacks

| # | What | Cause |
|---|---|---|
| F1 | Lines missing from the Tax Schedule: Form 1040 Other income (460.37); Sch A Real estate taxes (−2,928.00), Personal property taxes (−1,190.39); Sch B Interest income (2,945.26); 1099-R IRA federal (−18,000.00) and state (−7,000.00) tax withheld; 1099-G State and local tax refunds (4,070.00). Tax Item blank on those categories in the Tax Summary. | **Data.** The categories have no tax line. The QIF carries Quicken's tax code on each category (`R4416` on `Tax:Real Estate`); the import keeps only the `T` flag (phase-9 note, gap 13). |
| F2 | 1099-R Total IRA taxable distrib.: Quicken 325,824.15, Kansha 125,007.65. The 2025-12-15 Roth conversion (200,816.50, Vanguard LT IRA 156 → TX Acct → Vanguard Roth IRA 677) is missing. | **Data or engine**; see §4.3. |
| F3 | A group's total sits on its heading line; no closing "Total …" line. Kansha's closing lines also read "Total Total IRA taxable distrib.". | Layout. |
| F4 | Negative amounts in red. | Layout. |
| F5 | One line per row; long text cut with "…". | Layout. |
| F6 | Forms in order Form 1040, Schedule A, Schedule B, 1099-R, 1099-G. Kansha puts 1099-R first. | Built-in `tax_line.sort_order`. |
| F7 | Tax Summary bar: **Subtotal by** and **Sort by**. | Missing feature. |
| F8 | Tax Summary defaults: Last year, sort Account/Date. Kansha: Year to date, Date. | Defaults. |
| F9 | "S" marks a split transaction. | Missing column. |

## 3. Decisions (Stan, 2026-10-02)

- D1. Existing categories get their tax lines from a **one-time
  command** that reads the QIF; it never overwrites a tax line already
  set. Future imports set them too.
- D2. Total on the heading line is a Display option for every report,
  **on by default for the two tax reports only**.
- D3. Tax Summary Subtotal by: **Category** (default), Tax line,
  Account, Payee, Tag, Month, Quarter, Year, None.
- D4. The Roth conversion (F2) is investigated and designed in this
  plan.

Choices made in this design (Stan may overrule):

- D5. Red negatives apply to **every report**, as the register already
  does (`.neg`); the minus sign stays (not color alone).
- D6. Truncation is on screen only; print and PDF wrap, since paper has
  no hover.
- D7. The split marker is a new column `split` ("S"), on in the tax
  reports, off elsewhere.

## 4. Design

### 4.1 Quicken tax codes (F1; D1) — CAT-050, MIG-020

**Code table.** New `crates/kansha-core/src/import/tax_codes.rs`: a
`const` table of Quicken code → built-in `(form, line)` (migration
0003 names, which are Quicken's). From Stan's QIF; ✓ = confirmed by
the Quicken 2025 Tax Schedule:

| Code | Quicken categories | Form | Line | |
|---|---|---|---|---|
| 4112 | Misc Income, Other Inc | Form 1040 | Other income, misc. | ✓ |
| 8336 | Tax:Fed Est | Form 1040 | Fed. estimated tax, quarterly | |
| 4416 | Tax:Real Estate | Schedule A | Real estate taxes | ✓ |
| 8560 | Tax:Personal Property | Schedule A | Personal property taxes | ✓ |
| 4480 | Charity:Cash | Schedule A | Cash charity contributions | ✓ |
| 7760 | Charity:Noncash | Schedule A | Non-cash charity contributions | ✓ |
| 8352 | Tax:State Est | Schedule A | State estimated tax, quarterly | |
| 4400 | Tax:State Due | Schedule A | State income taxes | |
| 4592 | Interest Inc(ome), _IntInc, _Accrued Int | Schedule B | Interest income | ✓ |
| 4576 | Div Income | Schedule B | Dividend income | ✓ |
| 8512 | Tax:Fed IRA | 1099-R | IRA federal tax withheld | ✓ |
| 8528 | Tax:State IRA | 1099-R | IRA state tax withheld | ✓ |
| 4160 | Tax:State Refund, Tax Refund, State Return | 1099-G | State and local tax refunds | ✓ |
| 7664 | Unemployment Inc | 1099-G | Unemployment compensation | |
| 4256 | Social Security Income | SSA-1099 | Net social security benefits | |
| 9776 | Tax:Fed SS | SSA-1099 | Federal tax withheld | |
| 7808 | _LT/_MT/_ST CapGnDst | 1099-DIV | Total capital gain distr. | |
| 7360 | Salary, Bonus, _401Contrib, … | W-2 | Salary or wages | |
| 7376 | Tax:Fed | W-2 | Federal tax withheld | |
| 7392 | Tax:Soc Sec | W-2 | Social Security tax withheld | |
| 7424 | Tax:State | W-2 | State tax withheld | |
| 7680 | Tax:Medicare | W-2 | Medicare tax withheld | |

Not mapped (no Kansha line; reported, left alone): 8096 (spouse W-2
wages), 7824 (tax-free accrued interest). Codes not in the table are
reported the same way. Note `_ST CapGnDst` maps to 1099-DIV here,
while Kansha's built-in short-term distribution category is on
Schedule B (migration 0003); built-in (system) categories are never
changed by the codes.

**Parser.** `import/qif.rs`: `QifCategory` gains `tax_code:
Option<u32>` from the `R` field (`R4416` → 4416; anything else →
`None`). Test with the `R` lines above.

**Import (new books).** `import/plan.rs` carries the code into the
planned category; `import/commit.rs` (`category_at`) sets
`tax_line` on a category it **creates** when the code maps. A
category that already exists keeps what it has. The import result
gains counts: tax lines set, codes not mapped.

**One-time command (existing books).** Tools > Categories gear menu:
**Set tax lines from QIF…**

- Picks a QIF (`pick_import_file`), reads only its `!Type:Cat` list
  (`qif::parse` already reads the whole file; use its categories).
- Core: `categories::tax_lines_from_qif(conn, &[QifCategory]) ->
  TaxLinePlan` (preview) and `apply_tax_lines(tx, &TaxLinePlan)`.
  Match by full path (`Parent:Child`), ignoring case. For each:
  *set* (category has no tax line, code maps), *kept* (already has
  one — never overwritten), *unmapped code*, *no such category*,
  *system category* (skipped).
- Preview dialog lists each group with category and line; **Apply**
  writes the "set" ones in one `Db::write`, an audited update per
  category (AUD-010), after a **bulk backup** (BAK-020).
- Also sets `tax_related` on those categories (a tax line implies it,
  as the Tax Summary already treats it).

New IPC commands: `tax_lines_from_qif_preview(path)`,
`tax_lines_from_qif_apply(path)`; types `TaxLinePlan`,
`TaxLinePlanItem`, `TaxLinePlanStatus`. **⚠ API change.**

### 4.2 Form order (F6) — CAT-050, RPT-145

Migration **0010** (data only): new `sort_order` for the built-in tax
lines, keeping each form's lines in their current relative order:

| Form | Order |
|---|---|
| Form 1040 | 100s |
| Schedule A | 200s |
| Schedule B | 300s |
| Schedule D (computed, `SCHEDULE_D_ORDER`) | 350 |
| 1099-DIV | 400s |
| W-2 | 500s |
| SSA-1099 | 600s |
| 1099-R | 700s |
| 1099-G | 800s |
| 1099-SA, Form 8889 | 900s |

Rows are matched by `(form, line)`; user rows (if any) are untouched.
`reports/tax.rs`: `SCHEDULE_D_ORDER` = 350 and its comment.
**⚠ Schema change** (data migration). Test in `migrations.rs`: orders
after migrating from 9; forms come out in the table's order.

### 4.3 Roth conversion on 1099-R (F2) — CAT-050, RPT-145

What Quicken holds (`VangLTIRA156.QIF`, `VangRothIRA677.QIF`):

```
LT IRA 156   12/15'25  WithdrwX  Roth conversion   200,816.50  [TX Acct]
Roth IRA 677 12/15'25  ContribX  Self              200,816.50  [TX Acct]
```

`TX Acct` is a cash account used only to pass money between IRAs (it
also carries the 2024-10-01 conversion from Vanguard IRA 540). Quicken
puts the withdrawal on 1099-R because LT IRA 156's "transfers out" tax
line is 1099-R Total IRA taxable distrib.; the row's account is TX
Acct.

Kansha reaches a transfer tax line from a **cash transfer** out of an
account whose `tax_line_out` is set (`facts::lines`, `Want::TaxLines`).
Step 1 decides which case Stan's book is in:

**Step 1 — check the book (Stan, in the app; no code).**

1. Accounts: does **TX Acct** exist (not skipped at import)?
2. Vanguard LT IRA 156 → Edit Account Details: is *Transfers out* set
   to 1099-R Total IRA taxable distrib.? Same for Vanguard IRA 540
   (2024).
3. LT IRA 156 register, 12/15/2025: is the 200,816.50 a Cash Out to
   [TX Acct], or a Cash Out against Opening Balance?

**Case A — TX Acct exists, transfer is linked.** Only the account
setting is missing: set LT IRA 156 (and IRA 540) *Transfers out* to
1099-R Total IRA taxable distrib. No code. The report then shows the
row as Quicken does (account TX Acct, category [Vanguard LT IRA 156]).
Add a regression test anyway: a cash out from an IRA with
`tax_line_out` to a cash account lands on 1099-R.

**Case B — TX Acct was skipped**, so each leg is a Cash In/Out against
Opening Balance (phase-9 decision 2/3) and no transfer exists. Engine
change: money leaving an account with `tax_line_out` counts on that
line when the other side is **Opening Balance** (equity) — the import's
stand-in for a skipped account — for the investment actions Cash Out
and banking transfers only (not fees, not buys). Row: account = the IRA,
category = "Opening Balance". Test: the same conversion imported with
TX Acct skipped reaches 1099-R once; a fee does not; Case A rows are
not counted twice. Spec: CAT-050 and §18 report rules. Alternatively,
re-import is not an option (the book has edits since); Stan could
instead re-enter the two legs by hand as transfers through a new TX
Acct, then Case A applies — this design prefers the engine rule only if
Stan does not want to edit.

### 4.4 Totals on the heading line (F3; D2) — RPT-020

- `ReportSettings` gains `totals_on_heading: Option<bool>` (serde
  default `None`). `None` resolves to *on* for `TaxSchedule` and
  `TaxSummary`, *off* for the rest, so saved reports keep working and
  saved tax reports get the new look. Resolution in core
  (`ReportSettings::totals_on_heading()`), sent with the report as
  `Report.totals_on_heading: bool`.
- `src/lib/reports/rows.ts` `flatten(rows, isCollapsed, onHeading)`:
  when on, an expanded group's heading carries `r.cells` and no
  closing line is pushed; the OVERALL TOTAL row (a `Total` kind row,
  not a group) is unchanged.
- Customize > Display: checkbox **Totals on group heading**.
- CSV export follows the same setting (heading row has the totals; no
  "Total …" rows).
- Fixes "Total Total IRA taxable distrib." for the tax reports. For
  other reports with the option off, the closing label drops a leading
  duplicate: a label that already starts with "Total " gets no second
  "Total ".
- **⚠ API change** (`ReportSettings`, `Report`).
- Tests: `rows.test.ts` (both modes, collapsed groups, nested
  groups); `ReportWindow.test.ts` (checkbox round-trips); Rust:
  default resolution per kind; saved-report JSON without the field
  loads.

### 4.5 Red negatives (F4; D5) — RPT-050

`ReportTable.svelte`: a money cell whose text starts with "-" gets
class `neg` (same color token as the register's `.neg`); printing
keeps it (Quicken prints red). Minus sign stays. Test in
`ReportWindow.test.ts`: negative cell has the class, positive does
not.

### 4.6 One line per row (F5; D6)

`ReportTable.svelte`: text columns (`kind: text`) get
`white-space: nowrap; overflow: hidden; text-overflow: ellipsis` with a
`max-width` per column id (Account, Description, Memo, Category, Tax
Item; Date, Num, Clr, Amount fit), and `title` = full text when cut.
`@media print` drops the truncation. Per Stan's UI conventions: no
wraps on screen, no mismatched heights.

### 4.7 Split marker (F9; D7)

- `reports/facts.rs`: `Line` gains `split: bool` (the transaction has
  more than one category or transfer line); `cell("split")` → "S" or
  "".
- `tax.rs::columns()` and `itemized::columns(TaxSummary)` add
  `("split", "S", Text)` after Num; Itemized Categories and Payees add
  it hidden (`hidden_columns` default for those kinds).
- Test: a split transaction's rows show "S"; a simple one does not.

### 4.8 Tax Summary: Subtotal by and Sort by (F7; D3) — RPT-140

- New `text_enum` `TaxGroup` in `reports/mod.rs`: `category`,
  `tax_line`, `account`, `payee`, `tag`, `month`, `quarter`, `year`,
  `none`. `ReportSettings.tax_group: TaxGroup` (serde default
  `category`). Kept apart from `Subtotal` (Capital Gains) so neither
  report shows the other's choices.
- `reports/itemized.rs` (`By::TaxSummary`), after `facts::lines`:
  - `category`: as today (INCOME / EXPENSES / TRANSFERS, category
    tree, transfers by account), OVERALL TOTAL.
  - every other choice: no INCOME/EXPENSES/TRANSFERS sections (as
    Quicken); top-level groups, then OVERALL TOTAL:
    - `tax_line`: form → line, same order as the Tax Schedule;
      lines with no tax line (tax-related only) last under "(No tax
      line)". Schedule D is not added (it is not a category line).
    - `account`: the line's account, account-list order.
    - `payee`: reuse `payee_groups`.
    - `tag`: the Tag cell text; "(No tag)" last.
    - `month` / `quarter` / `year`: period of the line's date, in date
      order, labeled like the other reports' periods
      (`range.rs` helpers).
    - `none`: the lines, sorted, then OVERALL TOTAL.
  - Drill on each group: category, account as today; others none.
- Report bar (Tax Summary only): **Subtotal by** select (`tax_group`)
  and **Sort by** select (`sort` + `sort_desc`: Date, Account/Date,
  Amount, Num, each with descending), next to Date range, as Quicken.
  Both also stay in Customize; the column-heading sort still works.
- **⚠ API change** (`ReportSettings.tax_group`, type `TaxGroup`).
- Tests (Rust, `reports.rs`): each grouping's group labels, order,
  and totals on a small book; OVERALL TOTAL equals the category
  grouping's for every choice; filters still apply. TS: bar selects
  call `change` with the field.

### 4.9 Tax Summary defaults (F8) — RPT-140

`ReportSettings::defaults(TaxSummary)`: range `LastYear`, sort
`AccountDate`. New reports only; saved ones keep theirs. Test in
`reports.rs`.

## 5. Plan

Order: biggest effect first; each step ends with `just check` green
and a commit message for Stan.

| Step | Work | Sections | Flags |
|---|---|---|---|
| 1 | Quicken tax codes: parser, table, import, one-time command + dialog | 4.1 | ⚠ API |
| 2 | Stan runs *Set tax lines from QIF…* on his book; checks F2 Step 1 | 4.1, 4.3 | — |
| 3 | Totals on heading, red negatives, one line per row | 4.4–4.6 | ⚠ API |
| 4 | Tax Summary Subtotal by / Sort by, defaults | 4.8, 4.9 | ⚠ API |
| 5 | Form order (migration 0010) | 4.2 | ⚠ Schema |
| 6 | Split marker | 4.7 | — |
| 7 | Roth conversion: Case A test only, or Case B engine rule | 4.3 | — / spec §18 |

Steps 3–6 are independent of each other; 7 waits on Stan's Step 1.

Each step: spec edits (CAT-050, MIG-020, RPT-020, RPT-140, RPT-145,
§18 as touched), Appendix A entry, `just bindings` when the API
changes, and a section in `phase-notes/phase-9.md` (files, decisions,
gaps). Spec 0.7.4 for the set, or one minor bump per committed step.

## 6. Acceptance (against Quicken 2025)

After steps 1–2 and Case A/B, Tax Schedule for Last year:

| Form / line | Quicken | Kansha expected |
|---|---|---|
| Form 1040 Other income, misc. | 460.37 | 460.37 |
| Sch A Real estate taxes | −2,928.00 | −2,928.00 |
| Sch A Personal property taxes | −1,190.39 | −1,190.39 |
| Sch A Cash charity contributions | −20.00 | −20.00 |
| Sch A Non-cash charity contributions | −55,193.84 | −55,193.84 (Form 8283 needed) |
| Sch B Interest income | 2,945.26 | 2,945.26 plus investment interest, if any |
| Sch B Dividend income | 13.19 | 16,304.70 (includes investment dividends; by design) |
| Sch D | — | 28,135.40 (by design) |
| 1099-R Total IRA taxable distrib. | 325,824.15 | 325,824.15 |
| 1099-R IRA federal tax withheld | −18,000.00 | −18,000.00 |
| 1099-R IRA state tax withheld | −7,000.00 | −7,000.00 |
| 1099-G State and local tax refunds | 4,070.00 | 4,070.00 |

Tax Summary for Last year, Subtotal by Category: the same categories
as Quicken's, each with its Tax Item filled; Loan Payment:Interest
appears (tax-related in the QIF, no code); TRANSFERS shows Fidelity
IRA 510 (125,007.65) and Vanguard LT IRA 156 (200,816.50).

## 7. Open items

- Interest from investment accounts goes to Sch B Interest income via
  the built-in Interest category; Quicken's figure is banking only.
  Expect Kansha's to be higher if investment interest exists.
- 2/28 and 3/31/2025 Vanguard Grandma 140 rows show action "Misc
  Income" where Quicken shows "Div" (category Dividends is right).
  Check the import's action mapping (phase 9) separately.
- Spouse W-2 lines (code 8096) have no Kansha line; add one only if
  needed.
