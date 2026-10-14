# Budgets — build plan (#24)

Expands RPT-300 [Later] (spec 0.7.93). Written 2026-10-13 from Stan's
description and four Quicken 2013 screenshots
(`C:\Users\stanb\Downloads\scanned\quicken-budget-*.png`). Spec text
(new §12.5, BUD-xxx) is written once §4 is decided. Four steps; Stan
tries each in the app before the next.

## 1. As described by Stan

- New menu **Budget** on the menu bar (File, Edit, Tools, Reports,
  Budget, Help). First item: **Create Budget…**: a dialog asks for the
  name, then the budget view opens on it.
- Budget view, controls at the top, left to right:
  1. **View**: Graph View | Annual View.
  2. Graph View: **Period**: Monthly, Quarterly, Yearly, Quarter to
     date, Year to date, Last 3 months, Last 6 months.
     Annual View: **Year** with ◀ ▶.
  3. Graph View, Monthly/Quarterly/Yearly only: the period with ◀ ▶
     (current month, quarter, or year first). The four "to date" and
     "Last" choices have no third control.
     Annual View: **Show**: Details, Budget, Balance, Actual.
- Moving ◀ ▶ to a period whose year has no budget asks "No budget for
  2027. Create one?".
- **Annual View**: a spreadsheet. Rows: categories in two groups,
  Expenses and Income (the category's Kind). Columns: one per month
  (Details: Budget, Actual, Balance per month; otherwise one), then a
  year column: Balance for Details and Balance, Budget for Budget,
  Actual for Actual.
  Each month's Budget heading has a ▾ menu:
  - Copy March budget to all future months in 2026
  - Copy March budget to all past months in 2026
  - Copy March budget to all months in 2026
- **Graph View**: one horizontal bar per category: actual so far
  against the budget, green with money left, red when over. Right of
  the bars: dollars left (or over) in the period, then the period's
  budget.

## 2. What the Quicken screenshots add

- **Summary band** under the controls, all views: "SPENDING $111,818"
  over the range ("Jan 2026 - Oct 2026"), one wide bar of spending
  against the budget, "Budget: $124,969" above its right end,
  "$13,151 LEFT" under the bar. Annual View: the whole year.
- **Budget Actions ▾** at the top right (Kansha: the view's gear in
  the title band). Stan, 2026-10-13, its items:
  - Select categories to budget…
  - Budget reports ▸ Current budget, Historical budgets
  - Copy this budget…
  - Edit budget name…
  - Delete this budget…
  - Create new budget…
- **Whole dollars** everywhere in the view.
- **Rows are a flat list of budgeted categories**, not the tree:
  the path in plain text, the last name bold ("Car: BlueForester:
  **Gas**"). The group row (▼ PERSONAL EXPENSES, collapsible) carries
  the group's totals. Spending in categories not in the budget goes
  to an **Everything Else** row at the group's end (actual only;
  ▸ opens it to list them).
- **Annual View, Details**: past and current months show Budget,
  Actual, Balance; future months show Budget only. After the current
  month, a **To Date** column (Balance; Budget in Budget only).
  The current month is shaded. Last column "2026 Summary". Actual
  figures are blue (they open the transactions). Balance is bold,
  negative red.
- **Row hover** (Annual): a copy-across icon at the row label, and
  each month's cell becomes an edit box with a gear (row options).
- **Graph View**: the bar's full width is the period's budget (YTD:
  Books 81 spent of 89, "8 left"). The spent amount is written in the
  bar. Over budget: the whole bar red, the part past the budget
  lighter. "N left" / "N over" (red) / "N under" (a negative budget,
  red). Right column: Budget for the period. Row hover: the budget is
  an edit box for the period, with copy, gear, and a history chart
  icon.
- **Negative budgets** occur (Refund: budget −9, actual −7, "2
  under").
- Groups in Quicken are its category groups (Personal Expenses,
  Taxes). Kansha has none; it uses Kind (Expenses, Income) as Stan
  wrote.

## 3. Proposed model

- A **budget** has a name and covers whole years. A year exists in a
  budget once created (Create Budget makes the current year).
- A budget lists its **categories** (income and expense, any level).
  Create Budget starts with every category that had actual in the
  last 12 months, each month at last year's monthly average (§5.2).
  Amounts: one per budget, category, and month, `Money` in cents,
  signed (a negative budget is allowed for a refund category).
  Missing = 0.
- Stored positive for normal flow, both kinds. Actual:
  - Expense: spending, positive; a refund lowers it (as RPT-210).
  - Income: money received, positive.
  - Per split posting to a category; transfers and Opening Balance
    (equity) left out; all accounts, investment accounts included
    (dividends count as income).
  - A budgeted category's actual is its own postings plus those of
    its subcategories that are not budgeted themselves (so
    "Food" catches Food:Snacks unless Food:Snacks has its own row).
    Everything else goes to Everything Else.
- Balance = Budget − Actual for expenses (positive: money left).
  Income: Q3.
- Display: whole dollars, rounded half-even in Rust (as SET-080).
  Entry accepts cents; stored as typed.
- Graph bar: fill = actual ÷ period budget, as permille, computed in
  Rust; over budget: full width, split at budget ÷ actual. Budget ≤ 0:
  no fill, text only.
- All money math (sums, balances, period roll-ups, To Date, bar
  fractions) in `kansha-core`. The UI draws what it gets.

### Schema (next free migration: 0020, or 0021 after loans #23) ⚠ Schema change

```sql
CREATE TABLE budget (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL UNIQUE COLLATE NOCASE
        CHECK (length(trim(name)) > 0),
    created_at TEXT NOT NULL CHECK (...)
) STRICT;

CREATE TABLE budget_year (
    budget_id INTEGER NOT NULL REFERENCES budget (id) ON DELETE CASCADE,
    year      INTEGER NOT NULL CHECK (year BETWEEN 1900 AND 9999),
    PRIMARY KEY (budget_id, year)
) STRICT;

CREATE TABLE budget_category (
    budget_id   INTEGER NOT NULL REFERENCES budget (id) ON DELETE CASCADE,
    category_id INTEGER NOT NULL REFERENCES category (id),
    PRIMARY KEY (budget_id, category_id)
) STRICT;

CREATE TABLE budget_amount (
    budget_id   INTEGER NOT NULL,
    year        INTEGER NOT NULL,
    month       INTEGER NOT NULL CHECK (month BETWEEN 1 AND 12),
    category_id INTEGER NOT NULL,
    amount      INTEGER NOT NULL CHECK (amount <> 0),
    PRIMARY KEY (budget_id, year, category_id, month),
    FOREIGN KEY (budget_id, year)
        REFERENCES budget_year (budget_id, year) ON DELETE CASCADE,
    FOREIGN KEY (budget_id, category_id)
        REFERENCES budget_category (budget_id, category_id)
        ON DELETE CASCADE
) STRICT;
```

Zero amounts are deleted, not stored. `audit_log` rebuilt to accept
entity `budget` (as 0013, 0014, 0019). Category delete/merge: a merge
adds the source's amounts into the target's; a delete removes them
(the delete already asks).

## 4. Steps

1. **Core** (tests first): next free migration; `budget` module:
   create, rename, delete, choose categories, add year (Q2), set
   cell, set a period's budget from the Graph View (spread over its
   months, remainder to the last month), copy month (future / past /
   all), copy row across; actuals by category and month for a year in
   one pass (NFR-040: time on the large book with `just perf`); the
   Annual grid, Graph rows, and summary band built in Rust.
2. **IPC**: commands `budget_list`, `budget_create`, `budget_copy`,
   `budget_rename`, `budget_delete`, `budget_categories_set`,
   `budget_year_create`, `budget_set`, `budget_set_period`,
   `budget_copy_month` (with a category, a row's copy: no separate
   `budget_copy_row`), `budget_annual`, `budget_graph`;
   `just bindings`. ⚠ API change.
3. **UI, Annual View**: Budget menu, Create dialog, view on a sheet
   with the title band and gear, controls, summary band, grid
   (Details / Budget / Balance / Actual, To Date, Summary, shaded
   current month, collapsible groups, Everything Else), editable
   Budget cells (Enter/Tab move as a spreadsheet), ▾ heading menu,
   row copy-across, create-year prompt. Actual opens the transactions
   (as report drill-down).
4. **UI, Graph View**: period controls, bars, left/over/under and
   Budget columns, period budget edit on hover, create-year prompt.

## 5. Questions for Stan

Settled by the screenshots: parent categories (flat list of budgeted
categories, path shown), bar scale (the period's budget), graph rows
(budgeted categories plus Everything Else), per-row copy (yes).

Decided 2026-10-13 (all):

1. **More than one budget.** Yes. The gear holds the Budget Actions
   items (§2). The Budget menu: Create Budget…, then the budgets by
   name (open one). Copy this budget… asks a new name and copies
   categories and every year's amounts.
2. **Starting categories and amounts.** Categories with actual in the
   last 12 months. Each month starts at last calendar year's monthly
   average (year's actual ÷ 12, whole dollars, half-even); 0 when the
   category had none last year.
3. **Income balance.** Budget − Actual, as expenses.
4. **New year.** The prompt offers: copy this year's categories and
   budget values, or copy only this year's categories (amounts 0).
5. **Last 3 / Last 6 months.** Full months, as reports' "Last"
   presets. A year with no budget counts as 0, no prompt.
6. **Group order.** Expenses, then Income; each group's totals on its
   heading row. The last row: Total (Income − Expenses).
7. **Summary band.** Expenses only, as Quicken.
8. **Accounts.** All accounts. The user chooses categories only.
9. **History chart icon.** Left out.
10. **Budget reports.** Later; the gear shows them disabled until
    built.

## 6. Not in this plan

- Budget reports (Current budget, Historical budgets).
- History chart from the Graph View row.
- A Budget card on Insights.
- Rollover of unspent balance to the next month.
- Category groups (Quicken's Personal Expenses, Taxes).
- Budget-by-account or by tag.
