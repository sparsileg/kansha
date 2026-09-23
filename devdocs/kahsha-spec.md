# Kansha — Requirements, Functional Specification & Design

| | |
|---|---|
| **Document version** | 0.1 (draft) |
| **Target release** | Kansha 1.0.0 |
| **Last updated** | 2026-09-23 |
| **Owner** | Stan |
| **Status** | Draft — restructured from initial notes; contains recommendations pending decision |

---

## Part I — Overview

### 1. Purpose and Scope

Kansha is a lightweight, local, single-user personal finance application intended to replace Quicken 2013 for Windows. Version 1.0.0 must allow Stan to:

1. Import existing Quicken data (income/expense history, account structure, categories).
2. Enter and manage day-to-day income/expense transactions, including robust recurring transactions.
3. Reconcile accounts against statements.
4. Import or seed current investment holdings (lots, cost basis, acquisition dates) and track investment activity going forward.
5. Produce the core reports needed for spending review, net worth, tithing, and tax preparation.

Once 1.0.0 is stable and trusted, additional reports and modules may be added. The architecture must allow this without structural rewrites.

### 2. Document Conventions

**Requirement IDs.** Each requirement has a stable ID of the form `AREA-NNN` (e.g., `ACCT-030`). IDs are assigned in steps of 10 so new requirements can be inserted between existing ones (e.g., `ACCT-035`). IDs are never reused or renumbered; a withdrawn requirement is marked **[Withdrawn]** and kept for traceability.

**Release tags.**
- **[1.0]** — required for version 1.0.0
- **[Later]** — deferred; the design must not preclude it
- **[TBD]** — release not yet decided

**Source tags.**
- **[S]** — from Stan's original notes or stated in discussion
- **[R]** — Claude's recommendation, pending Stan's decision

Once Stan accepts a recommendation, its tag changes from [R] to [S].

**Placeholders** are marked `⟨PLACEHOLDER: …⟩` and listed in Part V.

### 3. Goals, Priorities, and Non-Goals

**Priorities (in order)** [S]
1. Data accuracy
2. Data protection (no loss, no corruption, no silent changes)
3. Traceability — every number can be explained by drilling down to the transactions that produced it
4. Usability for daily entry
5. Extensibility

**Goals for 1.0.0** [S]
- Replace Quicken 2013 for Stan's actual usage: banking/credit card registers, ~40 categories, recurring transactions, reconciliation, investment tracking with lots and cost basis.
- Run on Kubuntu and Windows; do not preclude macOS.
- Store data in an open, inspectable format (SQLite).

**Non-goals for 1.0.0** [S]
- Direct download/import from financial institutions (OFX/QFX/Direct Connect). Modular design must allow adding later.
- Financial and tax planning (projections, Roth conversion modeling, etc.).
- Multi-user support, cloud sync, mobile apps.
- Multi-currency (USD only) [R].
- Bill pay, invoicing, business features.

### 4. Glossary

| Term | Meaning |
|---|---|
| **Account** | A container of value: bank, credit card, investment, asset, liability. |
| **Transaction** | A dated financial event consisting of one or more postings. |
| **Posting** | One line of a transaction affecting one account or category with a signed amount. Postings of a transaction sum to zero internally. |
| **Split** | A user-visible transaction divided across multiple categories/transfers. Implemented as multiple postings. |
| **Category** | Classification of income or expense (e.g., Groceries). Hierarchical. |
| **Tag** | Optional cross-cutting label (Quicken calls these "classes"/"tags"). |
| **Transfer** | Movement of money between two Kansha accounts. |
| **Security** | A tradable instrument: stock, ETF, mutual fund, bond, money market fund. |
| **Lot** | A quantity of a security acquired on a specific date at a specific cost basis. |
| **Cost basis** | Total cost of a lot for tax purposes, including fees. |
| **Position** | Aggregate holding of a security in an account (sum of open lots). |
| **Cleared status** | Unmarked, Cleared (c), or Reconciled (R). |
| **Scheduled transaction** | A recurring or one-time future transaction template. |
| **Tax treatment** | Taxable, tax-deferred, or tax-exempt. |

---

## Part II — Functional Requirements

### 5. Accounts (ACCT)

#### 5.1 Account types

- **ACCT-010** [1.0][S] Support these account types: Checking, Savings, Credit Card, Cash, Money Market, Brokerage (taxable), Traditional IRA, Roth IRA, HSA, Other Asset, Other Liability.
- **ACCT-020** [1.0][R] Add account types 401(k)/403(b) and Loan/Mortgage (liability). Loan accounts in 1.0 are balance-tracking only; amortization schedules are [Later] (see REC-200).
- **ACCT-030** [1.0][R] Each account has a **tax treatment** of Taxable, Tax-Deferred, or Tax-Exempt, defaulted by type:
  - Traditional IRA, 401(k) → Tax-Deferred
  - Roth IRA, HSA (qualified use) → Tax-Exempt
  - All others → Taxable

  > **Correction to original notes:** the draft listed Roth IRA as tax-deferred. Roth accounts are tax-exempt (qualified withdrawals are tax-free), which matters for tax reports and investment income classification.

#### 5.2 Account attributes

- **ACCT-100** [1.0][S] Common attributes for all accounts: name, description, account type, financial institution, account number, contact phone, home page URL, tax treatment, opening date, visibility flags (show in account bar, show in account list), open/closed status.
- **ACCT-110** [1.0][S] Checking/Savings/Money Market additional attributes: interest rate.
- **ACCT-120** [1.0][S] Credit Card additional attributes: credit limit.
- **ACCT-130** [1.0][S] Investment accounts (Brokerage, IRA, Roth, HSA, 401(k)) additional attributes: account subtype, cash handling mode (see INV-300).
- **ACCT-140** [1.0][S] Other Asset additional attributes: asset subtype (house, vehicle, other), optional link to an associated liability account (mortgage or loan).
- **ACCT-150** [1.0][R] Account numbers are displayed masked (last 4 digits) by default, with an explicit reveal action.
- **ACCT-160** [1.0][R] Each account has an optional free-form notes field.

#### 5.3 Account lifecycle

- **ACCT-200** [1.0][S] Create, edit, and close accounts via an account edit modal.
- **ACCT-210** [1.0][R] **Closing** an account hides it from default views and blocks new transactions but retains all history. Closing requires a zero balance (and no open positions for investment accounts), or an explicit confirmation to close with a non-zero balance.
- **ACCT-220** [1.0][R] **Deleting** an account is only permitted if it has no transactions. Otherwise the user must close it. This protects history (see Section 13).
- **ACCT-230** [1.0][S] Account balance is always derived from transactions, never stored as authoritative data (see Section 15.1).
- **ACCT-240** [1.0][R] Accounts can be reordered and grouped (Banking, Credit, Investments, Retirement, Assets, Liabilities) in the account list.

### 6. Categories, Payees, and Tags (CAT, PAY, TAG)

#### 6.1 Categories

- **CAT-010** [1.0][S] Categories are either Income or Expense and support subcategories (at least two levels; recommend unlimited depth [R]).
- **CAT-020** [1.0][S] Create, rename, move (re-parent), and merge categories. Merging reassigns all postings from the source to the target and is recorded in the audit log.
- **CAT-030** [1.0][R] Categories cannot be deleted while in use; they can be hidden/archived.
- **CAT-040** [1.0][R] Category flags:
  - **Tax-related** (used by tax summary reports)
  - **Tithable income** (income counted for tithing calculation)
  - **Charitable giving / tithe** (giving counted against tithing)
- **CAT-050** [Later][R] Map categories to tax form lines (Schedule A, B, D, etc.).
- **CAT-060** [1.0][R] Built-in system categories for investment income and transfers (Dividends, Interest, Capital Gains Distributions, Realized Gain/Loss, Investment Fees) that cannot be deleted.

#### 6.2 Payees

- **PAY-010** [1.0][R] Maintain a payee list built from entered and imported transactions.
- **PAY-020** [1.0][R] **Memorized payees:** a payee may store a default category, tag, memo, and amount. Typing a known payee in the register auto-fills these (equivalent to Quicken QuickFill).
- **PAY-030** [1.0][R] Rename and merge payees (with audit logging).
- **PAY-040** [Later][R] Payee renaming rules for imported transactions (e.g., "COSTCO WHSE #1234" → "Costco").

#### 6.3 Tags

- **TAG-010** [1.0][S] Transactions and individual split lines may carry zero or more tags.
- **TAG-020** [1.0][R] Tags can be created, renamed, merged, and hidden.
- **TAG-030** [1.0][R] Reports can filter and group by tag.

### 7. Transactions and Register (TXN, REG)

#### 7.1 Transaction content

- **TXN-010** [1.0][S] A banking transaction has: date, payee, amount (payment or deposit), category or transfer account, tag(s), memo, check number, cleared status, notes.
- **TXN-020** [1.0][S] **Splits:** a transaction may be split across multiple categories and/or transfers, each line with its own amount, category, tag, and memo. Split lines must sum to the transaction total; the UI must show any unassigned remainder and prevent saving until it is resolved.
- **TXN-030** [1.0][S] **Transfers:** a transfer between two Kansha accounts is a single transaction appearing in both registers. Editing either side updates both; deleting either side deletes (or voids) the whole transaction.
- **TXN-040** [1.0][R] **Void:** a transaction can be voided (amount zeroed, marked VOID, original values preserved in the audit log) as an alternative to deletion.
- **TXN-050** [1.0][R] Deleting or editing a **Reconciled** transaction requires explicit confirmation and is recorded in the audit log with before/after values.
- **TXN-060** [Later][S] Attachments (receipts, statements) linked to transactions.
- **TXN-070** [1.0][R] Every transaction has an immutable internal ID and creation timestamp, and records its origin (manual entry, import batch ID, scheduled transaction ID).

#### 7.2 Register view

- **REG-010** [1.0][S] Register columns: Date, Num (check number) [R], Payee, Payment/Charge, Deposit [R], Category, Tag, Memo, Clr, Balance.
  > **Recommendation:** separate Payment and Deposit columns (as in Quicken) rather than a single signed column; this reduces sign errors during entry. Stan's draft had a combined Charge/Payment column — decision needed (D-10).
- **REG-020** [1.0][R] Running balance is computed in date order (tie-broken by entry order) and reflects all transactions up to that row.
- **REG-030** [1.0][R] Inline entry and editing at the bottom of the register, keyboard-driven: Tab moves between fields; Enter saves; Esc cancels; `+`/`-` adjusts date; `t` sets today.
- **REG-040** [1.0][R] Sort by any column; filter by date range, payee, category, tag, cleared status, and text search.
- **REG-050** [1.0][R] Split transactions show "--Split--" in the Category column with an expander to view/edit lines.
- **REG-060** [1.0][R] Footer shows current balance, cleared balance, and (for credit cards) available credit.
- **REG-070** [1.0][R] Future-dated transactions are visually distinguished and a line separates today from future entries.
- **REG-080** [1.0][R] Right-click/context menu: edit, split, void, delete, go to other side of transfer, show audit history.

### 8. Scheduled and Recurring Transactions (REC) and Calendar (CAL)

Stan uses this heavily in Quicken; it must be robust and cover all common patterns.

#### 8.1 Schedule definition

- **REC-010** [1.0][S] Each scheduled transaction has: payee, account, amount, category (or split), tag, memo, method (payment/deposit/transfer), next due date, frequency, end condition, and reminder lead time (days before due to notify).
- **REC-020** [1.0][S] Supported frequencies:
  - Only once
  - Daily [R]
  - Weekly; every N weeks (covers bi-weekly)
  - Twice a month on two specified days (e.g., 1st and 15th)
  - Monthly on a specific day; every N months
  - Monthly on the last day of the month [R]
  - Monthly on the Nth weekday (e.g., second Tuesday) [R]
  - Quarterly
  - Twice a year
  - Yearly; every N years
- **REC-030** [1.0][S] End conditions: never, on a specific end date, or after N occurrences ("# left," e.g., for loans). The remaining count decrements as occurrences are entered.
- **REC-040** [1.0][R] Day-of-month overflow handling: if a scheduled day does not exist in a month (e.g., the 31st), use the last day of that month.
- **REC-050** [1.0][R] Weekend/holiday adjustment option per schedule: none, move to previous business day, or move to next business day. 1.0 uses weekends only; a US bank holiday calendar is [Later].
- **REC-060** [1.0][R] Amount type: fixed, or estimated (the user confirms the actual amount when entering).
- **REC-070** [1.0][R] Entry mode per schedule: **Remind** (user must confirm entry) or **Auto-enter** (entered automatically on the due date, flagged for review). Default: Remind.

#### 8.2 Schedule operations

- **REC-100** [1.0][S] Create, view, edit, and delete schedules from a dedicated Scheduled Transactions list and from the calendar.
- **REC-110** [1.0][R] For a single upcoming occurrence: **Enter** (with optional edits to amount/date), **Skip**, or **Edit this occurrence only** without changing the series.
- **REC-120** [1.0][R] Editing a schedule offers "this occurrence only" vs. "this and all future occurrences."
- **REC-130** [1.0][R] On app startup, show a "Due and Overdue" list of occurrences within their reminder windows, including any missed while the app was closed. Nothing is ever silently entered for past dates without appearing in this list.
- **REC-140** [1.0][R] Create a schedule from an existing transaction ("Schedule this").
- **REC-150** [1.0][R] Scheduled transfers and scheduled splits are supported.
- **REC-160** [1.0][R] Each entered transaction records the schedule and occurrence it came from.
- **REC-200** [Later][R] Loan schedules that split each payment into principal and interest from an amortization table.

#### 8.3 Scheduled transaction list

- **REC-300** [1.0][S] List columns: Date Due, Payee, Amount, Account, Method, Frequency ("How often"), Remind Days, # Left, End Date, Mode.

#### 8.4 Calendar

- **CAL-010** [1.0][S] Month calendar view showing upcoming scheduled occurrences on their due dates.
- **CAL-020** [1.0][S] Toggle to also show completed (entered) transactions.
- **CAL-030** [1.0][R] Click a day to see its items; enter, skip, or edit occurrences directly from the calendar; create a new schedule on a chosen date.
- **CAL-040** [1.0][R] Filter the calendar by account(s).
- **CAL-050** [1.0][R] Optional projected daily balance for a selected account, based on current balance plus scheduled items.
- **CAL-060** [Later][R] Week and agenda (list) views.

### 9. Reconciliation (RCN)

- **RCN-010** [1.0][S] Reconcile any banking, credit card, or cash-bearing investment account against a statement.
- **RCN-020** [1.0][R] Reconcile workflow:
  1. Enter statement ending date, ending balance, and (optionally) interest earned and service charges, which are created as transactions.
  2. Display uncleared and cleared transactions up to the statement date, split into payments and deposits.
  3. User checks off items; the app shows the running cleared balance and the difference from the statement.
  4. **Finish** is enabled only when the difference is zero; on finish, all checked items become Reconciled (R).
- **RCN-030** [1.0][R] The opening balance shown at reconciliation start must equal the prior statement's ending balance. If it doesn't (because a reconciled transaction was changed), the app shows which reconciled transactions changed since the last reconciliation.
- **RCN-040** [1.0][R] If the user cannot resolve a difference, the app may create an explicit, clearly labeled **Balance Adjustment** transaction only after confirmation. It is never created automatically.
- **RCN-050** [1.0][R] Save and resume an in-progress reconciliation.
- **RCN-060** [1.0][R] Keep a reconciliation history per account (date, statement balance, items reconciled) viewable later.
- **RCN-070** [Later][R] Share/position reconciliation against brokerage statements.

### 10. Investments

#### 10.1 Security master (SEC)

- **SEC-010** [1.0][S] Maintain a list of securities with: name, ticker symbol, security type (stock, ETF, mutual fund, bond, money market fund, CD, other), asset class (e.g., US Equity, International Equity, Bond, Cash), and notes.
- **SEC-020** [1.0][R] Optional CUSIP field (helps match brokerage cost-basis reports).
- **SEC-030** [1.0][R] Per-security default cost-basis method (see LOT-100).
- **SEC-040** [1.0][R] Securities can be hidden but not deleted while referenced by any transaction.
- **SEC-050** [Later][R] Multiple asset-class allocations per security (e.g., a balanced fund that is 60% equity/40% bond).

#### 10.2 Prices (PRC)

- **PRC-010** [1.0][S] Store historical prices per security (date, closing price) separately from the security master.
- **PRC-020** [1.0][R] Manual price entry and editing.
- **PRC-030** [1.0][R] Import prices from CSV and from Quicken QIF price history (`!Type:Prices`).
- **PRC-040** [TBD][S] Download of current and historical prices from an online source.
  > **Recommendation:** make this [1.0] but pluggable behind a price-provider interface, since free quote sources change or disappear. Candidate sources need evaluation (D-40). Manual/CSV entry remains the fallback.
- **PRC-050** [1.0][R] Market value uses the most recent price on or before the valuation date; reports show the price date used and flag stale prices (older than a configurable number of days).

#### 10.3 Investment transactions (INV)

- **INV-010** [1.0][S] Supported investment transaction types:

  | Type | Effect on shares | Effect on cash | Effect on basis/income |
  |---|---|---|---|
  | Buy | + (new lot) | − | Lot basis = cost + fees |
  | Sell | − (from lots) | + | Realized gain/loss |
  | Dividend | none | + | Dividend income |
  | Interest | none | + | Interest income |
  | Reinvest dividend | + (new lot) | none | Dividend income + new lot |
  | Reinvest capital gain (ST/LT) | + (new lot) | none | CG distribution income + new lot |
  | Capital gain distribution (cash, ST/LT) [R] | none | + | CG distribution income |
  | Return of capital | none | + | Reduces lot basis |
  | Stock split / reverse split | ± quantity | none | Per-share basis adjusted; total basis and dates unchanged |
  | Transfer shares in/out | ± | none | Lots transferred with original dates and basis |
  | Shares added / removed (opening position) [R] | ± | none | Creates a lot with user-supplied date and basis |
  | Transfer cash in/out | none | ± | none |
  | Fee | none | − | Investment expense |
  | Tax withholding (federal/foreign) | none | − | Recorded for tax reporting |
  | Miscellaneous income/expense [R] | none | ± | Categorized |

- **INV-020** [1.0][R] Each investment transaction records trade date and, optionally, settlement date.
- **INV-030** [1.0][R] Entry forms are type-specific (e.g., Sell prompts for lot selection); the register shows Date, Action, Security, Quantity, Price, Commission, Amount, Cash Balance.
- **INV-040** [1.0][R] Every investment transaction that affects cash also produces the corresponding ledger postings, so investment income appears in income/expense reports.
- **INV-050** [1.0][R] Money market funds held as "cash" can be treated either as a security (with $1.00 price) or as the account's cash balance, configurable per account (D-50).

#### 10.4 Cash handling

- **INV-300** [1.0][S] Each investment account has a cash handling mode:
  - **Internal cash** — the account holds its own cash balance (default).
  - **Linked cash account** — cash effects post to a designated checking/money-market account ("show cash in checking account" in the original notes).
- **INV-310** [1.0][R] Negative cash balances are allowed but flagged.

#### 10.5 Lots and cost basis (LOT)

- **LOT-010** [1.0][S] Every acquisition (buy, reinvest, shares added, transfer in) creates a lot with: security, account, acquisition date, quantity, and total cost basis.
- **LOT-020** [1.0][S] Sales reduce lots according to the lot-selection method; partial lot sales split the lot, allocating basis proportionally.
- **LOT-030** [1.0][R] Rounding: when basis is split, cents are allocated so the parts always sum exactly to the original basis (no penny drift).
- **LOT-040** [1.0][R] Each realized gain/loss record stores: sale date, acquisition date, quantity, proceeds, basis, gain/loss, and holding period (short-term if held one year or less, long-term otherwise).
- **LOT-100** [1.0][S] Lot selection methods: **FIFO** and **Specific Identification**. Default method configurable per account and per security; overridable on each sale.
- **LOT-110** [TBD][R] **Average cost** method for mutual funds. Needed if either brokerage reports average cost for any of Stan's funds (D-60).
- **LOT-120** [1.0][R] Stock splits adjust quantity and per-share basis of every open lot while preserving acquisition dates and total basis.
- **LOT-130** [1.0][R] Return of capital reduces basis across open lots pro rata by quantity; if basis would go below zero, the excess is a capital gain.
- **LOT-140** [1.0][R] Share transfers between accounts carry lots intact.
- **LOT-150** [1.0][R] Lot view per account/security: open lots with acquisition date, quantity, basis, per-share basis, market value, unrealized gain/loss, holding period.
- **LOT-160** [1.0][R] Tax-deferred and tax-exempt accounts track lots the same way (for record-keeping), but gains from those accounts are excluded from taxable gain reports.
- **LOT-200** [Later][S] Wash sale detection and basis adjustment.
- **LOT-210** [Later][S] Inherited shares (stepped-up basis, automatic long-term holding).
- **LOT-220** [Later][R] Spinoffs, mergers, and other corporate actions.
- **LOT-230** [Later][R] Roth contribution basis tracking.

#### 10.6 Positions and derived values (POS)

- **POS-010** [1.0][S] Derive per account and across accounts: shares held, cost basis, market value, unrealized gain/loss, realized gain/loss, income by security.
- **POS-020** [1.0][S] Asset allocation by asset class, across all or selected accounts.
- **POS-030** [TBD][S] Investment performance.
  > **Recommendation:** 1.0 includes simple measures (total gain, total return including income) per security and account. Time-weighted and money-weighted (IRR) returns are [Later].
- **POS-040** [1.0][S] Investment account tabs: Overview | Transactions | Holdings | Lots | Income | Performance.
- **POS-050** [1.0][R] Share balance per security must equal the sum of open lot quantities at all times (integrity invariant; see INT-030).

### 11. Data Migration from Quicken (MIG)

This section is intentionally incomplete until export testing is done (P-01 through P-05).

#### 11.1 Known facts and assumptions

- Stan's Quicken 2013 holds about 5–6 years of active data (older data archived) [S].
- Quicken 2013 can export QIF and QXF [S].
- QIF is a documented plain-text format covering accounts, categories, classes/tags, banking transactions, investment transactions, securities, and prices [R].
- QXF is Quicken's proprietary transfer format; no reliable public specification is known [R].
- QIF does not reliably carry specific-lot assignments for past sales, so open lots should be seeded from brokerage cost-basis reports, not reconstructed from Quicken history [S/R].
- Quicken scheduled transactions may not export in any usable format and may need manual re-entry ⟨PLACEHOLDER P-04⟩ [R].

#### 11.2 Requirements

- **MIG-010** [1.0][S] Import income/expense history (banking and credit card accounts) from Quicken. ⟨PLACEHOLDER P-01: source format (QIF vs. QXF) to be decided after export testing.⟩
- **MIG-020** [1.0][S] Import the category list, including hierarchy.
- **MIG-030** [1.0][R] Import tags/classes.
- **MIG-040** [1.0][R] All imports go through a **staging area**: parse → preview → map/resolve → commit. Nothing touches the live ledger until the user commits.
- **MIG-050** [1.0][R] Preview shows counts by account, date range, and per-account totals, plus any warnings (unparseable lines, unknown categories, ambiguous dates).
- **MIG-060** [1.0][R] Mapping step: map unknown categories to existing categories or create them; map Quicken account names to Kansha accounts.
- **MIG-070** [1.0][R] Transfers exported from both sides (e.g., the checking side and the savings side) are matched and imported once, not twice.
- **MIG-080** [1.0][R] Each import commit is atomic and tagged with an import batch ID; a whole batch can be rolled back.
- **MIG-090** [1.0][R] Imported cleared/reconciled status is preserved.
- **MIG-100** [1.0][R] **Verification after import:** compare Kansha account balances as of the export date and category totals by year against Quicken reports. Stan exports the Quicken reports; Kansha provides matching report layouts so comparison is direct. ⟨PLACEHOLDER P-02: which Quicken reports to export as the reference.⟩
- **MIG-110** [1.0][S] Investment data: seed current open lots per taxable account.
  ⟨PLACEHOLDER P-03: seeding source — brokerage cost-basis CSV (recommended), Quicken QIF per account, or manual entry.⟩
- **MIG-120** [1.0][R] Lot seeding via CSV template (account, security, acquisition date, quantity, cost basis), with preview and validation, creating "shares added" transactions dated at the seeding date with original acquisition dates preserved on the lots.
- **MIG-130** [1.0][S] Tax-deferred/exempt accounts may be seeded with position totals only (quantity and total basis per security) rather than lot detail.
- **MIG-140** [1.0][R] Import securities list and price history from QIF where available.
- **MIG-150** [1.0][R] Scheduled transactions: ⟨PLACEHOLDER P-04: import if Quicken exports them usably; otherwise manual re-entry, supported by a "Quicken schedule checklist" to confirm all were recreated.⟩
- **MIG-160** [1.0][R] Known QIF pitfalls the parser must handle: two-digit-year and apostrophe date formats (e.g., `1/5'26`), locale-dependent date order, amounts with commas, split lines (`S`/`E`/`$`), bracketed transfer categories (`[Account Name]`), category/tag syntax (`Category/Tag`), and memorized-transaction sections that must not be imported as transactions.
- **MIG-170** [1.0][R] Keep original import files in a protected import archive alongside the database for audit purposes.
- **MIG-200** [Later][S] OFX/QFX/CSV import of new transactions from institutions, with duplicate detection and matching (shares the staging area of MIG-040).

### 12. Reports and Dashboard (RPT, DSH)

#### 12.1 General report features

- **RPT-010** [1.0][S] Reports offer both tables and graphs where meaningful.
- **RPT-020** [1.0][S] Report settings (date range, accounts, categories, tags, grouping, columns) can be saved as named reports and rerun.
- **RPT-030** [1.0][S] Every number in a report can be drilled into to show the contributing transactions (traceability principle).
- **RPT-040** [1.0][R] Date range presets: this month, last month, YTD, last year, last 12 months, custom; plus comparison to a prior period.
- **RPT-050** [1.0][R] Export to CSV and PDF; print.

#### 12.2 Reports in 1.0

- **RPT-100** [1.0][S] **Spending/Income by category** — for a period, with subcategory rollup; optionally by month (columns) to show trends.
- **RPT-110** [1.0][S] **Net worth** — as of a date and over time (graph), by account group.
- **RPT-120** [1.0][S] **Account balances/status** — all or selected accounts as of a date.
- **RPT-130** [1.0][S] **Tithing report** — tithable income (CAT-040 flagged categories) × configurable percentage, versus giving recorded, with balance, for a chosen period.
- **RPT-140** [1.0][S] **Tax summary** — totals of tax-related categories, investment income (dividends, interest, capital gain distributions), taxable realized gains (short/long-term), and withholdings, for a tax year. This supports tax estimation; it does not compute tax (planning is out of scope).
- **RPT-150** [1.0][R] **Realized gains detail** — lot-level sales for a period, suitable for checking against broker Form 1099-B.
- **RPT-160** [1.0][R] **Investment income** — by security and account, for a period.
- **RPT-170** [1.0][R] **Holdings/portfolio value** — positions, market value, basis, unrealized gain, as of a date.
- **RPT-180** [1.0][S] **Asset allocation** — table and chart.
- **RPT-190** [1.0][R] **Cash flow** — inflows vs. outflows by month, excluding transfers between own accounts.
- **RPT-200** [1.0][R] **Transaction report** — filtered list of transactions (general-purpose query tool).
- **RPT-300** [Later][S] Budgets and budget-vs-actual reports.
- **RPT-310** [Later][R] Performance reports (TWR/IRR).

#### 12.3 Dashboard

- **DSH-010** [1.0][S] Household dashboard showing net worth with breakdown (Investments, Cash, Other assets, Liabilities) and this month's income, expenses, and net.
- **DSH-020** [1.0][R] Upcoming scheduled transactions (next 14 days, configurable) and overdue items.
- **DSH-030** [1.0][R] Warnings panel: stale prices, unreconciled accounts beyond a threshold, integrity check results, last backup age.

### 13. Data Integrity, Audit, Backup, and Security (INT, AUD, BAK, SECU)

#### 13.1 Integrity

- **INT-010** [1.0][S] The transaction ledger is the single source of truth; balances, positions, and gains are derived.
- **INT-020** [1.0][R] All changes that touch multiple records (e.g., a transfer, a sale consuming several lots) are applied in a single database transaction: all or nothing.
- **INT-030** [1.0][R] **Integrity check** (on demand and on a schedule, e.g., at startup) verifies invariants:
  - every transaction's postings sum to zero
  - both sides of every transfer exist and match
  - share balances equal the sum of open lot quantities
  - lot basis totals equal acquisitions minus basis consumed by sales and adjustments
  - reconciled balances match reconciliation history
  - SQLite `PRAGMA integrity_check` passes
- **INT-040** [1.0][R] Integrity failures are reported with specific records identified; the app never auto-repairs silently.
- **INT-050** [1.0][R] Derived values may be cached for performance, but caches are always rebuildable from the ledger and never treated as authoritative.

#### 13.2 Audit trail

- **AUD-010** [1.0][S] Every create, edit, void, and delete of transactions, lots, accounts, categories, payees, and schedules is recorded in an append-only audit log: timestamp, action, entity ID, before and after values, and origin (UI, import batch, scheduler).
- **AUD-020** [1.0][R] View audit history for any transaction or account from the UI.
- **AUD-030** [1.0][S] Corrections to historical data are explicit and visible, never silent.

#### 13.3 Backup and restore

- **BAK-010** [1.0][S] Backups are a first-class feature.
- **BAK-020** [1.0][R] Automatic backup on application close and before any import, schema migration, or bulk operation (merge, batch rollback).
- **BAK-030** [1.0][R] Manual "Back up now" with user-chosen destination.
- **BAK-040** [1.0][R] Configurable retention (e.g., keep last 10 automatic backups plus one per month for 12 months).
- **BAK-050** [1.0][R] Backups are consistent snapshots (SQLite online backup API or `VACUUM INTO`), never a raw file copy of an open database.
- **BAK-060** [1.0][R] Backups remain encrypted with the same mechanism as the live database.
- **BAK-070** [1.0][R] Restore from backup: preview backup date and summary, back up the current database first, then restore.
- **BAK-080** [1.0][R] Each backup is verified after writing (open, integrity check).

#### 13.4 Security

- **SECU-010** [1.0][R] Database encrypted at rest using **SQLCipher** (decision pending final confirmation; D-20).
- **SECU-020** [1.0][R] The encryption passphrase is known to the user so the database can be opened in DB Browser for SQLite (SQLCipher build). Optional storage in the OS keyring (KWallet on Kubuntu, Windows Credential Manager) for convenience.
- **SECU-030** [1.0][R] Clear warning at setup: a lost passphrase makes the database and all backups unrecoverable. The user is prompted to record it durably (e.g., password manager).
- **SECU-040** [1.0][R] Change passphrase function (re-keys the database).
- **SECU-050** [1.0][R] External browsing is supported read-only. Direct edits via external tools are unsupported; the integrity check (INT-030) will detect resulting inconsistencies.
- **SECU-060** [1.0][R] Optional auto-lock after a configurable idle period.
- **SECU-070** [1.0][R] No network access except explicitly enabled features (price download). No telemetry.

### 14. User Interface and Settings (UI, SET)

#### 14.1 Navigation and layout

- **UI-010** [1.0][S] Account selector dropdown at the top of the main window; can be toggled to a persistent sidebar alongside the register.
- **UI-020** [1.0][S] Icon bar with user-configurable shortcuts (Reconcile, Investments, Calendar, Reports, Dashboard, Scheduled, etc.).
- **UI-030** [1.0][S] Account-centric design: each account opens to its own view with tabs appropriate to its type (banking: Register | Scheduled | Reconcile history; investment: see POS-040).
- **UI-040** [1.0][R] Multiple accounts/reports can be open in tabs within the main window.
- **UI-050** [1.0][R] Global keyboard shortcuts for common actions; full keyboard operation of the register.
- **UI-060** [1.0][R] Undo for the most recent edit in the current session (implemented as an explicit reversing change, recorded in the audit log).

#### 14.2 Settings

- **SET-010** [1.0][S] Multiple themes (at least light and dark).
- **SET-020** [1.0][S] Adjustable font size, applied globally.
- **SET-030** [1.0][R] Date display format; first day of week.
- **SET-040** [1.0][R] Default lot selection method; stale-price threshold; tithing percentage.
- **SET-050** [1.0][R] Backup location, retention, and schedule.
- **SET-060** [1.0][R] Startup behavior: open dashboard or last view; run integrity check at startup.
- **SET-070** [1.0][R] Settings are stored in the database (portable with the data), except window geometry and database path, which are stored locally per machine.

---

## Part III — Non-Functional Requirements (NFR)

- **NFR-010** [1.0][S] Platforms: Kubuntu 26.04 LTS and Windows 10/11. macOS must not be precluded (no platform-specific code outside the native layer).
- **NFR-020** [1.0][S] Single-user, local-only; data in a single SQLite database file.
- **NFR-030** [1.0][R] Monetary values are exact: no binary floating-point arithmetic anywhere in financial calculations (see Section 19).
- **NFR-040** [1.0][R] Performance: register with 10,000 transactions opens in under 1 second; typical reports render in under 2 seconds on Stan's hardware.
- **NFR-050** [1.0][R] Data volume: comfortably supports 20+ years of data (hundreds of thousands of transactions).
- **NFR-060** [1.0][R] Durability: SQLite WAL mode with `synchronous=FULL`; no data loss on application crash.
- **NFR-070** [1.0][R] Schema is documented and stable enough for external read-only inspection.
- **NFR-080** [1.0][R] Accessibility: keyboard navigation throughout; respects font-size setting; adequate contrast in all themes.
- **NFR-090** [1.0][R] Dates are calendar dates without time zones (financial dates never shift due to time zone conversion).

---

## Part IV — Design

### 15. Design Principles and Rationale

These principles are drawn from the original notes and discussion. They guide design decisions and resolve conflicts between requirements.

#### 15.1 The ledger is the source of truth
Balances are never stored as authoritative facts. The chain is:

```
Account → Transactions (postings) → Calculated balance
Investment Account → Security transactions → Lots → Positions → Market value / gain
```

**Rationale:** this makes it possible to reconstruct the state of any account at any historical date, and to answer "why does this account say $842,173.19?" by drilling down to the exact transactions. A stored balance can drift; a derived one cannot.

#### 15.2 Double-entry internally, simple on the surface
Every transaction is stored as postings that sum to zero. Categories are modeled internally as income/expense ledger accounts, so a grocery purchase is a posting of −$184.32 to Checking and +$184.32 to the Groceries category.

**Rationale:** double-entry guarantees money cannot be created or lost by a bug or a half-finished edit, and makes transfers and splits natural. The user sees a Quicken-style register, not debits and credits. Note that this is more rigorous than Quicken itself, which is essentially single-entry with linked transfers.

#### 15.3 Explicit corrections, never silent changes
Edits are logged with before/after values; destructive actions prefer void/close over delete; reconciliation adjustments require confirmation.

**Rationale:** trustworthiness depends on being able to see what changed and when.

#### 15.4 Start with the essential accounting engine
Tax lots with FIFO and specific identification come first; wash sales, inherited shares, and complex corporate actions are deferred.

**Rationale:** cost basis is the most complex part of the model. Getting the core right and tested before adding edge cases reduces risk.

#### 15.5 Account-centric, not screen-centric
Users navigate to an account and see everything about it (register, holdings, lots, income, performance) rather than switching between global screens.

#### 15.6 Modular with defined interfaces
Each module exposes a defined API and does not reach into other modules' storage directly. This allows later modules (budgets, OFX import, tax planning) without restructuring.

#### 15.7 Verify against authoritative sources
Seed investment lots from brokerage cost-basis reports rather than from Quicken; verify imported history against Quicken's own reports; run Kansha in parallel with Quicken until results agree.

### 16. Technology Stack

#### 16.1 Stack as proposed in original notes [S]

| Layer | Technology |
|---|---|
| UI structure/presentation | HTML + CSS |
| UI and application logic | TypeScript |
| Frontend build | Vite |
| Desktop framework | Tauri v2 |
| Native layer | Rust |
| Database | SQLite |
| Source control | Git + GitHub |

The overall choice is sound: Tauri is a good fit for a local, cross-platform app, SQLite is close to ideal for single-user finance data, and the stack matches Stan's existing projects.

#### 16.2 Recommendations and commentary [R]

**R1 — Put the accounting engine in Rust, not TypeScript (D-30).**
The original notes place domain logic (balances, lots, cost basis) in TypeScript and keep Rust as a thin native layer. Given that accuracy is the top priority, I recommend the reverse for the financial core:
- **Numeric safety:** JavaScript's `number` is binary floating point; exact money math in TypeScript requires discipline with `bigint` or a decimal library everywhere. Rust with integer minor units or `rust_decimal` makes exactness the default, enforced by the type system.
- **One transactional boundary:** if validation and posting logic live next to the database, every multi-record change (a sale consuming three lots and posting cash) is validated and committed atomically in one place. With logic in TypeScript, the frontend assembles changes and Rust just writes them, which makes it easier for a UI bug to write inconsistent data.
- **Testability:** the engine can be tested with `cargo test`, including property-based tests, without any UI.
- **Cost:** a larger IPC surface (commands like `post_transaction`, `sell_lots`, `run_report`). This is manageable with typed command definitions shared between Rust and TypeScript (e.g., generated with `specta`/`tauri-specta` or `ts-rs`).

TypeScript would then own UI state, forms, display formatting, and charts. Reports would be computed in Rust (SQL plus engine) and rendered in TypeScript.

**R2 — Reconsider "no UI framework" (D-35).**
Kansha's UI is more demanding than a typical utility: an editable, keyboard-driven register grid, split editors, a calendar, tabbed account views, modals, and many reports. Plain TypeScript can do this, but it tends to accumulate hand-written DOM update code. Since Photyx already uses Svelte with Tauri, **Svelte 5** would add little new learning and reduce UI code substantially while staying lightweight. If you prefer no framework, the document should define a small component pattern up front so the UI stays consistent.

**R3 — Database access and encryption.**
- `rusqlite` with the `bundled-sqlcipher` feature (single, consistent SQLite/SQLCipher version on all platforms).
- Schema migrations managed in Rust with a versioned migration table (e.g., `rusqlite_migration` or hand-rolled numbered SQL files).
- External browsing: DB Browser for SQLite, SQLCipher-enabled build.

**R4 — Supporting libraries (candidates, to evaluate).**
- Decimal math: `rust_decimal` (Rust).
- Dates: `chrono` or `time` in Rust, using date-only types; in TypeScript, keep dates as ISO `YYYY-MM-DD` strings and avoid the JavaScript `Date` object for financial dates.
- Charts: a lightweight library such as Chart.js, uPlot, or ECharts (evaluate for print/PDF export).
- PDF export: render report HTML to PDF through the webview's print facility, or a Rust PDF crate.
- Keyring: `keyring` crate (supports KWallet/Secret Service and Windows Credential Manager).
- Testing: `cargo test` + `proptest` (engine); Vitest (TypeScript); Playwright or WebdriverIO for end-to-end (optional in 1.0).

**R5 — Tauri security configuration.**
Use Tauri v2 capabilities to expose only Kansha's own commands to the frontend; disable remote content; keep the network permission limited to the price provider (if enabled).

### 17. Architecture

#### 17.1 Layers

```
┌───────────────────────────────────────────────┐
│ UI (TypeScript, optional Svelte)              │
│  Views, forms, register grid, calendar,       │
│  charts, formatting, UI state                 │
└───────────────────────┬───────────────────────┘
                        │ Typed Tauri commands (IPC)
┌───────────────────────▼───────────────────────┐
│ Application services (Rust)                   │
│  Command handlers, validation, orchestration  │
├───────────────────────────────────────────────┤
│ Domain engine (Rust)                          │
│  Ledger · Lots/cost basis · Scheduler ·       │
│  Reconciliation · Reports · Import            │
├───────────────────────────────────────────────┤
│ Persistence (Rust)                            │
│  Repositories, migrations, backup, audit log  │
└───────────────────────┬───────────────────────┘
                        │
                 ┌──────▼──────┐
                 │ SQLCipher DB │
                 └─────────────┘
```

#### 17.2 Modules [R]

| Module | Responsibilities | Depends on |
|---|---|---|
| `accounts` | Account CRUD, types, attributes, lifecycle | persistence |
| `ledger` | Transactions, postings, splits, transfers, voids, balances | accounts, categories |
| `categories` | Categories, payees, tags, memorized payees | persistence |
| `schedule` | Scheduled transactions, recurrence rules, occurrence generation | ledger |
| `reconcile` | Reconciliation sessions and history | ledger |
| `securities` | Security master, prices, price providers | persistence |
| `investments` | Investment transactions, lots, cost basis, positions | ledger, securities |
| `reports` | Report definitions, queries, saved reports | ledger, investments |
| `import` | Staging, parsers (QIF, CSV, later QXF/OFX), mapping, commit, rollback | ledger, investments, categories |
| `integrity` | Invariant checks | all read-only |
| `audit` | Append-only change log | persistence |
| `backup` | Snapshot, retention, verify, restore | persistence |
| `settings` | Preferences | persistence |

Rule: modules interact only through their public APIs; only `persistence` issues SQL against another module's tables.

### 18. Conceptual Data Model [R]

Entity outline (full schema to be specified in the next revision):

- **account** — id, type, name, attributes, tax_treatment, status, group, sort order
- **category** — id, parent_id, kind (income/expense/system), name, flags (tax-related, tithable, giving), hidden
- **payee** — id, name, memorized defaults
- **tag** — id, name, hidden
- **transaction** — id (immutable), date, payee_id, check_num, memo, status (normal/void), origin, import_batch_id, schedule_id, created_at
- **posting** — id, transaction_id, account_id *or* category_id, amount (integer cents), memo, cleared_status, reconcile_id
- **posting_tag** — posting_id, tag_id
- **security** — id, ticker, name, type, asset_class, cusip, default_lot_method
- **price** — security_id, date, price
- **investment_txn** — transaction_id, account_id, security_id, action, quantity, price, commission, trade_date, settle_date
- **lot** — id, account_id, security_id, acquired_date, quantity, cost_basis, origin_txn_id, status
- **lot_disposal** — id, lot_id, sale_txn_id, quantity, proceeds, basis, gain, term
- **lot_adjustment** — id, lot_id, txn_id, type (split, return of capital, transfer), details
- **schedule** — id, template (payee, account, amount, splits…), recurrence rule, next_due, end condition, remaining, remind_days, mode, weekend rule
- **schedule_occurrence** — schedule_id, due_date, status (pending/entered/skipped), txn_id
- **reconciliation** — id, account_id, statement_date, statement_balance, status, finished_at
- **import_batch** — id, source file, format, created_at, status (staged/committed/rolled back)
- **audit_log** — id, timestamp, entity, entity_id, action, before_json, after_json, origin
- **saved_report** — id, name, type, settings_json
- **setting** — key, value
- **schema_version** — version, applied_at

### 19. Numeric Precision and Rounding [R]

- **Money:** stored as 64-bit integers in cents.
- **Quantities (shares):** stored as scaled integers with 6 decimal places (covers mutual fund fractional shares; to be confirmed against brokerage precision, D-70).
- **Prices:** scaled integers with 6 decimal places.
- **Computation:** `rust_decimal` for intermediate calculations; rounding to cents only at defined points (posting creation, basis allocation), using round-half-even unless a specific rule requires otherwise.
- **Allocation:** when dividing an amount (basis across lots, return of capital), allocate remainders deterministically so parts sum exactly to the whole.
- **IPC:** amounts cross to TypeScript as integers or strings, never as floating-point numbers.

### 20. Testing and Verification Strategy [R]

1. **Engine unit tests** with hand-computed expected results, written before implementation. Example: buy 100 VTI @ $200, buy 50 @ $220, sell 75 FIFO @ $250 → basis $15,000, proceeds $18,750, gain $3,750; remaining lots 25 @ $200 and 50 @ $220.
2. **Property-based tests** for invariants: postings always sum to zero; share balances equal open lot sums; basis is conserved through splits and transfers; recurrence rules never skip or duplicate occurrences.
3. **Recurrence tests** covering month-end, leap years, Nth weekday, twice-monthly, and weekend adjustment.
4. **Import tests** using sample QIF files (synthetic plus sanitized excerpts of Stan's exports), including the pitfalls in MIG-160.
5. **Migration verification** (MIG-100): Kansha balances and category totals match Quicken reports exactly.
6. **Parallel run:** Kansha and Quicken used side by side for an agreed period (recommend 2–3 months including at least one full reconciliation cycle per account) before Quicken is retired.
7. **Backup/restore drills:** restore a backup into a scratch location and run the integrity check.

### 21. Versioning, Schema Migration, and Release [R]

- Application follows semantic versioning; 1.0.0 is the first release Stan relies on for real data.
- Database schema has its own version number; migrations run automatically at startup after an automatic backup (BAK-020), and are forward-only.
- The app refuses to open a database with a newer schema version than it supports.
- This document is versioned in Git alongside the code; significant changes are logged in Appendix A.

---

## Part V — Open Decisions and Placeholders

### Decisions needed

| ID | Decision | Recommendation |
|---|---|---|
| D-10 | Register: combined signed amount column or separate Payment/Deposit columns | Separate columns |
| D-20 | Encryption: SQLCipher vs. disk-level only | SQLCipher (tentatively accepted; final decision before development) |
| D-30 | Accounting engine in Rust vs. TypeScript | Rust |
| D-35 | UI framework: none vs. Svelte 5 | Svelte 5 |
| D-40 | Price download in 1.0, and which provider | Yes, pluggable; provider to be evaluated |
| D-50 | Money market funds: security or cash | Per-account option |
| D-60 | Average-cost basis needed in 1.0? | Depends on brokerage reporting for Stan's funds |
| D-70 | Share/price decimal precision | 6 decimal places, confirm with brokerage data |
| D-80 | Parallel-run duration before retiring Quicken | 2–3 months |
| D-90 | Confirm 1.0 report list (Section 12.2) | As listed |
| D-100 | Confirm account type list, including 401(k) and Loan/Mortgage | As listed in ACCT-010/020 |

### Placeholders

| ID | Placeholder | Needed to resolve |
|---|---|---|
| P-01 | Source format for income/expense history (QIF vs. QXF) | Export a small account in both formats and inspect them in a text editor |
| P-02 | Quicken reports used as reference for import verification | Choose reports (e.g., account balances as of export date; category totals by year) |
| P-03 | Source for seeding investment lots | Check each brokerage's cost-basis/unrealized-gain CSV export |
| P-04 | Whether Quicken scheduled transactions can be exported | Test export; otherwise plan manual re-entry |
| P-05 | Whether QIF exports from Quicken 2013 include categories, tags, securities, and prices in usable form | Test full QIF export |

---

## Appendix A — Change Log

| Version | Date | Changes |
|---|---|---|
| 0.1 | 2026-09-23 | Restructured from initial notes; added requirement IDs, release/source tags, recommendations, design rationale, open decisions, and migration placeholders. |
