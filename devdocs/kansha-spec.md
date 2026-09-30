# Kansha — Requirements, Functional Specification & Design

| | |
|---|---|
| **Document version** | 0.4 (draft) |
| **Target release** | Kansha 1.0.0 |
| **Last updated** | 2026-09-29 |
| **Owner** | Stan |
| **Status** | Draft — prototype built (Phases 0–8: schema, ledger engine, register UI, scheduling and calendar, reconciliation, investments, reports and dashboard, encryption, backup, restore, and settings) and reviewed (`devdocs/phase-notes/prototype-review.md`); D-20, D-40, D-50, D-60, D-100, D-110, D-120, D-140 decided. Next: Phase 9, Quicken import (MIG) |

---

## Part I — Overview

### 1. Purpose and Scope

Kansha is a lightweight, local, single-user personal finance
application intended to replace Quicken 2013 for Windows. Version
1.0.0 must allow Stan to:

1. Import existing Quicken data (income/expense history, account
   structure, categories).
2. Enter and manage day-to-day income/expense transactions, including
   robust recurring transactions.
3. Reconcile accounts against statements.
4. Import or seed current investment holdings (lots, cost basis,
   acquisition dates) and track investment activity going forward.
5. Produce the core reports needed for spending review, net worth,
   and tax preparation.

Once 1.0.0 is stable and trusted, additional reports and modules may
be added. The architecture must allow this without structural
rewrites.

### 2. Document Conventions

**Requirement IDs.** Each requirement has a stable ID of the form
`AREA-NNN` (e.g., `ACCT-030`). IDs are assigned in steps of 10 so new
requirements can be inserted between existing ones (e.g.,
`ACCT-035`). IDs are never reused or renumbered; a withdrawn
requirement is marked **[Withdrawn]** and kept for traceability.

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
3. Traceability — every number can be explained by drilling down to
   the transactions that produced it
4. Usability for daily entry
5. Extensibility

**Goals for 1.0.0** [S]
- Replace Quicken 2013 for Stan's actual usage: banking/credit card
  registers, ~40 categories, recurring transactions, reconciliation,
  investment tracking with lots and cost basis.
- Run on Kubuntu and Windows; do not preclude macOS.
- Store data in an open, inspectable format (SQLite).

**Non-goals for 1.0.0** [S]
- Direct download/import from financial institutions (OFX/QFX/Direct
  Connect). Modular design must allow adding later.
- Financial and tax planning (projections, Roth conversion modeling,
  etc.).
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

- **ACCT-010** [1.0][S] Support these account types: Checking,
  Savings, Credit Card, Cash, Money Market, Brokerage (taxable),
  Traditional IRA, Roth IRA, HSA, Other Asset, Other Liability.
- **ACCT-020** [1.0][S] Add account types 401(k)/403(b) and
  Loan/Mortgage (liability). Loan accounts in 1.0 are balance-tracking
  only; amortization schedules are [Later] (see REC-200).
- **ACCT-030** [1.0][R] Each account has a **tax treatment** of
  Taxable, Tax-Deferred, or Tax-Exempt, defaulted by type:
  - Traditional IRA, 401(k) → Tax-Deferred
  - Roth IRA, HSA (qualified use) → Tax-Exempt
  - All others → Taxable

  > **Correction to original notes:** the draft listed Roth IRA as
  > tax-deferred. Roth accounts are tax-exempt (qualified withdrawals
  > are tax-free), which matters for tax reports and investment income
  > classification.

#### 5.2 Account attributes

- **ACCT-100** [1.0][S] Common attributes for all accounts: name,
  description, account type, financial institution, account number,
  contact phone, home page URL, tax treatment, opening date, show in
  account list, open/closed status. (The navigation bar's buttons,
  accounts included, are chosen in Edit > Navigation Bar, UI-020;
  the schema's `account.show_in_bar` column is unused.)
- **ACCT-110** [1.0][S] Checking/Savings/Money Market additional
  attributes: interest rate.
- **ACCT-120** [1.0][S] Credit Card additional attributes: credit
  limit.
- **ACCT-130** [1.0][S] Investment accounts (Brokerage, IRA, Roth,
  HSA, 401(k)) additional attributes: account subtype, cash handling
  mode (see INV-300).
- **ACCT-140** [1.0][S] Other Asset additional attributes: asset
  subtype (house, vehicle, other), optional link to an associated
  liability account (mortgage or loan).
- **ACCT-150** [1.0][R] Account numbers are displayed masked (last 4
  digits) by default, with an explicit reveal action.
- **ACCT-160** [1.0][R] Each account has an optional free-form notes
  field.

#### 5.3 Account lifecycle

- **ACCT-200** [1.0][S] Create, edit, and close accounts via an
  account edit modal.
- **ACCT-210** [1.0][R] **Closing** an account hides it from default
  views and blocks new transactions but retains all history. Closing
  requires a zero balance (and no open positions for investment
  accounts), or an explicit confirmation to close with a non-zero
  balance.
- **ACCT-220** [1.0][R] **Deleting** an account is only permitted if
  it has no transactions. Otherwise the user must close it. This
  protects history (see Section 13).
- **ACCT-230** [1.0][S] Account balance is always derived from
  transactions, never stored as authoritative data (see Section 15.1).
- **ACCT-240** [1.0][R] Accounts can be reordered and grouped
  (Banking, Credit, Investments, Retirement, Assets, Liabilities) in
  the account list.

### 6. Categories, Payees, and Tags (CAT, PAY, TAG)

#### 6.1 Categories

- **CAT-010** [1.0][S] Categories are either Income or Expense and
  support subcategories (at least two levels; recommend unlimited
  depth [R]).
- **CAT-020** [1.0][S] Create, rename, move (re-parent), and merge
  categories. Merging reassigns all postings from the source to the
  target and is recorded in the audit log.
- **CAT-030** [1.0][R] Categories cannot be deleted while in use; they
  can be hidden/archived.
- **CAT-040** [1.0][R] Category flag **Tax-related** (used by tax
  summary reports). The tithable-income and charitable-giving flags
  are **[Withdrawn]** (0.3.22): giving is reviewed with existing
  reports. Their `category` columns stay in migration 0001, unused.
- **CAT-050** [1.0][S] Map categories to tax form lines (W-2, 1099-R,
  Schedule A, B, …; built-in list, migration 0003). An account maps
  transfers out of it and transfers into it to a line each (an IRA
  distribution to 1099-R). Schedule D comes from lot disposals, not a
  category.
- **CAT-060** [1.0][R] Built-in system categories for investment
  income and transfers (Dividends, Interest, Capital Gains
  Distributions, Realized Gain/Loss, Investment Fees) that cannot be
  deleted.

#### 6.2 Payees

- **PAY-010** [1.0][R] Maintain a payee list built from entered and
  imported transactions.
- **PAY-020** [1.0][R] **Memorized payees:** a payee may store a
  default category, tag, memo, and amount. Typing a known payee in the
  register auto-fills these (equivalent to Quicken QuickFill).
- **PAY-030** [1.0][R] Rename and merge payees (with audit logging).
- **PAY-040** [Later][R] Payee renaming rules for imported
  transactions (e.g., "COSTCO WHSE #1234" → "Costco").

#### 6.3 Tags

- **TAG-010** [1.0][S] Transactions and individual split lines may
  carry zero or more tags.
- **TAG-020** [1.0][R] Tags can be created, renamed, merged, and
  hidden.
- **TAG-030** [1.0][R] Reports can filter and group by tag.

### 7. Transactions and Register (TXN, REG)

#### 7.1 Transaction content

- **TXN-010** [1.0][S] A banking transaction has: date, payee, amount
  (payment or deposit), category or transfer account, tag(s), memo,
  check number, cleared status, notes.
- **TXN-020** [1.0][S] **Splits:** a transaction may be split across
  multiple categories and/or transfers, each line with its own amount,
  category, tag, and memo. Split lines must sum to the transaction
  total; the UI must show any unassigned remainder and prevent saving
  until it is resolved.
- **TXN-030** [1.0][S] **Transfers:** a transfer between two Kansha
  accounts is a single transaction appearing in both
  registers. Editing either side updates both; deleting either side
  deletes (or voids) the whole transaction.
- **TXN-040** [1.0][R] **Void:** a transaction can be voided (amount
  zeroed, marked VOID, original values preserved in the audit log) as
  an alternative to deletion.
- **TXN-050** [1.0][R] Deleting or editing a **Reconciled**
  transaction requires explicit confirmation and is recorded in the
  audit log with before/after values.
- **TXN-060** [Later][S] Attachments (receipts, statements) linked to
  transactions.
- **TXN-070** [1.0][R] Every transaction has an immutable internal ID
  and creation timestamp, and records its origin (manual entry, import
  batch ID, scheduled transaction ID).

#### 7.2 Register view

- **REG-010** [1.0][S] Register columns: Date, Num (check number) [R],
Payee, Payment/Charge, Deposit [R], Category, Tag, Memo, Clr, Balance.
> **Decided (D-10):** separate Payment and Deposit columns (as in
Quicken), which reduces sign errors during entry. The entry row has
both fields; typing an amount in one clears the other. Stan's draft
had a combined column.
- **REG-020** [1.0][R] Running balance is computed in date order
  (tie-broken by entry order) and reflects all transactions up to that
  row.
- **REG-030** [1.0][R] Inline entry and editing at the bottom of the
  register, keyboard-driven: Tab moves between fields; Enter saves;
  Esc cancels; `+`/`-` adjusts date; `t` sets today.
- **REG-040** [1.0][R] Sort by any column; filter by date range,
  payee, category, tag, and cleared status. Text search is the
  navigation bar search (UI-070), which can be limited to the open
  account.
- **REG-050** [1.0][R] Split transactions show "--Split--" in the
  Category column with an expander to view/edit lines. Their Tag
  column shows only the transaction's own tag; each line's tag shows
  on its line (the Tag filter still finds a split by a line's tag). A
  Split button (two arrows forking up from one point) before Enter turns the
  entry into a split, the category chosen so far becoming its first
  line; it is off while the split lines are open.
- **REG-060** [1.0][R] Footer shows current balance, cleared balance,
  and (for credit cards) available credit.
- **REG-070** [1.0][R] Future-dated transactions are visually
  distinguished and a line separates today from future entries. In
  every register (banking and investment) future rows are italic
  and their alternate rows use their own tint; their text is not
  dimmed. Reconciled rows have gray text.
- **REG-080** [1.0][R] Right-click/context menu: Edit, Split (opens
  the edit with its split lines, the category so far as line 1), Mark
  cleared or unmarked, Go to other side of transfer, Schedule this…
  (REC-140), History… (AUD-020), Void, Delete.

### 8. Scheduled and Recurring Transactions (REC) and Calendar (CAL)

Stan uses this heavily in Quicken; it must be robust and cover all
common patterns.

#### 8.1 Schedule definition

- **REC-010** [1.0][S] Each scheduled transaction has: payee, account,
  amount, category (or split), tag, memo, method
  (payment/deposit/transfer), next due date, frequency, end condition,
  and reminder lead time (days before due to notify).
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
- **REC-030** [1.0][S] End conditions: never, on a specific end date,
  or after N occurrences ("# left," e.g., for loans). The remaining
  count decrements as occurrences are entered or skipped.
- **REC-040** [1.0][R] Day-of-month overflow handling: if a scheduled
  day does not exist in a month (e.g., the 31st), use the last day of
  that month.
- **REC-050** [1.0][R] Weekend/holiday adjustment option per schedule:
  none, move to previous business day, or move to next business
  day. 1.0 uses weekends only; a US bank holiday calendar is [Later].
- **REC-060** [1.0][R] Amount type: fixed, or estimated (the user
  confirms the actual amount when entering).
- **REC-070** [1.0][R] Entry mode per schedule: **Remind** (user must
  confirm entry) or **Auto-enter** (entered automatically on the due
  date, flagged for review). Default: Remind.

#### 8.2 Schedule operations

- **REC-100** [1.0][S] Create, view, edit, and delete schedules from a
  dedicated Scheduled Transactions list and from the calendar.
- **REC-110** [1.0][R] For a single upcoming occurrence: **Enter**
  (with optional edits to amount/date), **Skip**, or **Edit this
  occurrence only** without changing the series.
- **REC-120** [1.0][R] Editing a schedule offers "this occurrence
  only" vs. "this and all future occurrences."
- **REC-130** [1.0][R] On app startup, show a "Due and Overdue" list
  of occurrences within their reminder windows, including any missed
  while the app was closed. Nothing is ever silently entered for past
  dates without appearing in this list.
- **REC-140** [1.0][R] Create a schedule from an existing transaction
  ("Schedule this").
- **REC-150** [1.0][R] Scheduled transfers and scheduled splits are
  supported.
- **REC-160** [1.0][R] Each entered transaction records the schedule
  and occurrence it came from. Deleting such a transaction gives the
  occurrence back: the schedule's latest one (remind mode, schedule not
  deleted) returns to Due, pending with its one-time edits, and "# left"
  counts it again; an ended schedule comes back. Otherwise it becomes
  skipped: an earlier one (occurrences are handled in order), an
  auto-entry one (it would only be entered again), or one of a deleted
  schedule.
- **REC-200** [Later][R] Loan schedules that split each payment into
  principal and interest from an amortization table.

#### 8.3 Scheduled transaction list

- **REC-300** [1.0][S] List columns: Date Due, Payee, Amount, Account,
  Method, Frequency ("How often"), Remind Days, # Left, End Date,
  Mode.

#### 8.4 Calendar

- **CAL-010** [1.0][S] Month calendar view showing upcoming scheduled
  occurrences on their due dates.
- **CAL-020** [1.0][S] Toggle to also show completed (entered)
  transactions: entered and skipped occurrences, and every register
  transaction on its date, scheduled or not (investment accounts and
  voids left out). A transfer between two cash accounts shows once per
  account, as each register does. The day dialog (CAL-030) always
  lists them.
- **CAL-030** [1.0][R] Click a day to see its items; enter, skip, or
  edit occurrences directly from the calendar; create a new schedule
  on a chosen date. Clicking a transaction, or double-clicking a day's
  blank space, opens a dialog listing that day's transactions:
  scheduled ones, open and done (entered or skipped), and register
  transactions (CAL-020), with Enter, Edit, Skip, and Close, and New
  Schedule set apart. Edit opens an entered or register transaction in
  its register, otherwise its schedule.
- **CAL-040** [1.0][R] Filter the calendar by account(s).
- **CAL-050** [1.0][R] Optional projected daily balance for a selected
  account, based on current balance plus scheduled items.
- **CAL-060** [Later][R] Week and agenda (list) views.

### 9. Reconciliation (RCN)

- **RCN-010** [1.0][S] Reconcile any banking, credit card, or
  cash-bearing investment account against a statement.
- **RCN-020** [1.0][R] Reconcile workflow:
  1. Enter statement ending date, ending balance, and (optionally)
     interest earned and service charges, which are created as
     transactions.
  2. Display uncleared and cleared transactions up to the statement
     date, split into payments and deposits.
  3. User checks off items; the app shows the running cleared balance
     and the difference from the statement.
  4. **Finish** is enabled only when the difference is zero; on
     finish, all checked items become Reconciled (R).
- **RCN-030** [1.0][R] The opening balance shown at reconciliation
  start must equal the prior statement's ending balance. If it doesn't
  (because a reconciled transaction was changed), the app shows which
  reconciled transactions changed since the last reconciliation.
- **RCN-040** [1.0][R] If the user cannot resolve a difference, the
  app may create an explicit, clearly labeled **Balance Adjustment**
  transaction only after confirmation. It is never created
  automatically.
- **RCN-050** [1.0][R] Save and resume an in-progress reconciliation.
- **RCN-060** [1.0][R] Keep a reconciliation history per account
  (date, statement balance, items reconciled) viewable later.
- **RCN-070** [Later][R] Share/position reconciliation against
  brokerage statements.

### 10. Investments

#### 10.1 Security master (SEC)

- **SEC-010** [1.0][S] Maintain a list of securities with: name,
  ticker symbol, security type (stock, ETF, mutual fund, bond, money
  market fund, CD, other), asset class (e.g., US Equity, International
  Equity, Bond, Cash), and notes.
- **SEC-020** [1.0][R] Optional CUSIP field (helps match brokerage
  cost-basis reports).
- **SEC-030** [1.0][R] Per-security default cost-basis method (see
  LOT-100).
- **SEC-040** [1.0][R] Securities can be hidden but not deleted while
  referenced by any transaction.
- **SEC-050** [Later][R] Multiple asset-class allocations per security
  (e.g., a balanced fund that is 60% equity/40% bond).

#### 10.2 Prices (PRC)

- **PRC-010** [1.0][S] Store historical prices per security (date,
  closing price) separately from the security master.
- **PRC-020** [1.0][R] Manual price entry and editing.
- **PRC-030** [1.0][S] Import a price list, started by hand (Tools >
  Import Prices…, or Import prices… in Securities): one price per line,
  ticker, price, and optionally a date as `MM/DD/YYYY`, separated by a
  comma or by spaces and tabs. The dialog has a date picker (default
  today); a line without a date takes that date. The file is chosen
  with a file picker or dropped on the dialog. A preview lists every
  line; tickers not in the book are skipped; nothing is stored unless
  every other line is good; a price already stored for that date is
  replaced. Quicken's QIF price history is read only by the Quicken
  import (MIG-140).
- **PRC-040** [1.0][S] Download the latest price of every security
  that is not hidden and has a ticker (Tools > Download Prices), dated
  the market day it belongs to. Off until turned on in Settings
  (SECU-070). The provider sits behind an interface, since free quote
  sources change or disappear: Yahoo Finance's chart service now (no
  key; stocks, ETFs, mutual funds), a keyed provider later (D-40). The
  status bar reports the prices stored and names the tickers that got
  none. Manual entry and PRC-030 remain the fallback.
- **PRC-050** [1.0][R] Market value uses the most recent price on or
  before the valuation date; reports show the price date used and flag
  stale prices (older than the stale-price setting, SET-040, unless the
  security sets its own): Holdings marks them ⚠ and says so under the
  report; the Investments screen and the dashboard warn too.

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

- **INV-020** [1.0][R] Each investment transaction records trade date
  and, optionally, settlement date.
- **INV-030** [1.0][R] Entry forms are type-specific (e.g., Sell
  prompts for lot selection); the register shows Date, Action,
  Security, Quantity, Price, Commission, Amount, Cash Balance.
- **INV-040** [1.0][R] Every investment transaction that affects cash
  also produces the corresponding ledger postings, so investment
  income appears in income/expense reports.
- **INV-050** [1.0][S] Money market funds held as "cash" can be
  treated either as a security (with $1.00 price) or as the account's
  cash balance, configurable per account (D-50, decided).

#### 10.4 Cash handling

- **INV-300** [1.0][S] Each investment account has a cash handling
  mode:
  - **Internal cash** — the account holds its own cash balance
    (default).
  - **Linked cash account** — cash effects post to a designated
    checking/money-market account ("show cash in checking account" in
    the original notes).
- **INV-310** [1.0][R] Negative cash balances are allowed but flagged.

#### 10.5 Lots and cost basis (LOT)

- **LOT-010** [1.0][S] Every acquisition (buy, reinvest, shares added,
  transfer in) creates a lot with: security, account, acquisition
  date, quantity, and total cost basis.
- **LOT-020** [1.0][S] Sales reduce lots according to the
  lot-selection method; partial lot sales split the lot, allocating
  basis proportionally.
- **LOT-030** [1.0][R] Rounding: when basis is split, cents are
  allocated so the parts always sum exactly to the original basis (no
  penny drift).
- **LOT-040** [1.0][R] Each realized gain/loss record stores: sale
  date, acquisition date, quantity, proceeds, basis, gain/loss, and
  holding period (short-term if held one year or less, long-term
  otherwise).
- **LOT-100** [1.0][S] Lot selection methods: **FIFO** and **Specific
  Identification**. Default method configurable per account and per
  security; overridable on each sale.
- **LOT-110** [1.0][R] **Average cost** method for mutual funds
  (D-60). Before a disposal, every open lot of the holding takes its
  share of the holding's total basis by shares (lot adjustments of
  kind `average`, migration 0004; they sum to zero), so each share
  carries the average; shares then leave oldest first, which sets the
  holding period.
- **LOT-115** [1.0][S] **HIFO** (highest cost first) and **minimum
  tax** lot selection. Minimum tax picks lots in this order, without
  needing tax rates: short-term losses, long-term losses, long-term
  gains (smallest first), short-term gains (smallest first). The
  engine resolves either method to specific lots at sale time and
  records them as ordinary disposals. HIFO ranks by basis per share,
  ties to the older lot. Minimum tax without a sale price (a share
  transfer, shares removed) takes the oldest lots first.
- **LOT-120** [1.0][R] Stock splits adjust quantity and per-share
  basis of every open lot while preserving acquisition dates and total
  basis.
- **LOT-130** [1.0][R] Return of capital reduces basis across open
  lots pro rata by quantity; if basis would go below zero, the excess
  is a capital gain.
- **LOT-140** [1.0][R] Share transfers between accounts carry lots
  intact.
- **LOT-150** [1.0][R] Lot view per account/security: open lots with
  acquisition date, quantity, basis, per-share basis, market value,
  unrealized gain/loss, holding period.
- **LOT-160** [1.0][R] Tax-deferred and tax-exempt accounts track lots
  the same way (for record-keeping), but gains from those accounts are
  excluded from taxable gain reports.
- **LOT-200** [Later][S] Wash sale detection and basis adjustment.
- **LOT-210** [Later][S] Inherited shares (stepped-up basis, automatic
  long-term holding).
- **LOT-220** [Later][R] Spinoffs, mergers, and other corporate
  actions.
- **LOT-230** [Later][R] Roth contribution basis tracking.

#### 10.6 Positions and derived values (POS)

- **POS-010** [1.0][S] Derive per account and across accounts: shares
  held, cost basis, market value, unrealized gain/loss, realized
  gain/loss, income by security.
- **POS-020** [1.0][S] Asset allocation by asset class, across all or
  selected accounts.
- **POS-030** [1.0][S] Investment performance for a period, by account
and security, in the Investment Performance report: starting and
ending value, net money in, income, gain, annual internal rate of
return (money-weighted, 365-day years), and the time-weighted return
for the whole period. A security's money in and out is its trades'
cash (reinvested income moves nothing); an account's is money crossing
its edge (cash in and out and register transfers, or with linked cash
every trade's cash); shares moved without cash count at market
value. The rest of an account (its cash, or with linked cash its other
income and fees) is its own row, so the rows add up to the account.
- **POS-040** [1.0][S] The Investments screen replaces per-account
  tabs. It lists the chosen investment accounts as collapsible rows;
  under each, its cash and current equities; under each equity, its
  open lots. Columns: Name (always), Ticker Symbol, Quote/Price,
  Shares, Cost Basis, Market Value, Gain/Loss, Day Gain/Loss, Price
  Day Change (%). A collapsed account shows rolled-up Cost Basis,
  Market Value, Gain/Loss, Day Gain/Loss, and Day %; a collapsed
  equity shows every column it has data for; an expanded row shows its
  children instead; Totals close the list. An as-of date (default
  today) sets the valuation; a date with no price uses the latest
  earlier price, and the day columns are blank unless a price is dated
  exactly that day and an earlier one exists. Ten named views
  (Default, Custom 2 to Custom 10, renamable) keep the columns and
  their order, the accounts and their order, and the equities shown;
  Reset View restores a view. An investment account opens as a
  register like a checking account's; typing in its empty line opens
  the entry dialog. Income, Performance, and realized gains move to
  the reports (Phase 7).
- **POS-050** [1.0][R] Share balance per security must equal the sum
  of open lot quantities at all times (integrity invariant; see
  INT-030).

### 11. Data Migration from Quicken (MIG)

This section is intentionally incomplete until export testing is done
(P-01 through P-05).

#### 11.1 Known facts and assumptions

- Stan's Quicken 2013 holds about 5–6 years of active data (older data
  archived) [S].
- Quicken 2013 can export QIF and QXF [S].
- QIF is a documented plain-text format covering accounts, categories,
  classes/tags, banking transactions, investment transactions,
  securities, and prices [R].
- QXF is Quicken's proprietary transfer format; no reliable public
  specification is known [R].
- QIF does not reliably carry specific-lot assignments for past sales,
  so open lots should be seeded from brokerage cost-basis reports, not
  reconstructed from Quicken history [S/R].
- Quicken scheduled transactions may not export in any usable format
  and may need manual re-entry ⟨PLACEHOLDER P-04⟩ [R].

#### 11.2 Requirements

- **MIG-010** [1.0][S] Import income/expense history (banking and
  credit card accounts) from Quicken. ⟨PLACEHOLDER P-01: source format
  (QIF vs. QXF) to be decided after export testing.⟩
- **MIG-020** [1.0][S] Import the category list, including hierarchy.
- **MIG-030** [1.0][R] Import tags/classes.
- **MIG-040** [1.0][R] All imports go through a **staging area**:
  parse → preview → map/resolve → commit. Nothing touches the live
  ledger until the user commits.
- **MIG-050** [1.0][R] Preview shows counts by account, date range,
  and per-account totals, plus any warnings (unparseable lines,
  unknown categories, ambiguous dates).
- **MIG-060** [1.0][R] Mapping step: map unknown categories to
  existing categories or create them; map Quicken account names to
  Kansha accounts.
- **MIG-070** [1.0][R] Transfers exported from both sides (e.g., the
  checking side and the savings side) are matched and imported once,
  not twice.
- **MIG-080** [1.0][R] Each import commit is atomic and tagged with an
  import batch ID; a whole batch can be rolled back.
- **MIG-090** [1.0][R] Imported cleared/reconciled status is
  preserved.
- **MIG-100** [1.0][R] **Verification after import:** compare Kansha
  account balances as of the export date and category totals by year
  against Quicken reports. Stan exports the Quicken reports; Kansha
  provides matching report layouts so comparison is
  direct. ⟨PLACEHOLDER P-02: which Quicken reports to export as the
  reference.⟩
- **MIG-110** [1.0][S] Investment data: seed current open lots per
  taxable account.  ⟨PLACEHOLDER P-03: seeding source — brokerage
  cost-basis CSV (recommended), Quicken QIF per account, or manual
  entry.⟩
- **MIG-120** [1.0][R] Lot seeding via CSV template (account,
  security, acquisition date, quantity, cost basis), with preview and
  validation, creating "shares added" transactions dated at the
  seeding date with original acquisition dates preserved on the lots.
- **MIG-130** [1.0][S] Tax-deferred/exempt accounts may be seeded with
  position totals only (quantity and total basis per security) rather
  than lot detail.
- **MIG-140** [1.0][R] Import securities list and price history from
  QIF where available.
- **MIG-150** [1.0][R] Scheduled transactions: ⟨PLACEHOLDER P-04:
  import if Quicken exports them usably; otherwise manual re-entry,
  supported by a "Quicken schedule checklist" to confirm all were
  recreated.⟩
- **MIG-160** [1.0][R] Known QIF pitfalls the parser must handle:
  two-digit-year and apostrophe date formats (e.g., `1/5'26`),
  locale-dependent date order, amounts with commas, split lines
  (`S`/`E`/`$`), bracketed transfer categories (`[Account Name]`),
  category/tag syntax (`Category/Tag`), and memorized-transaction
  sections that must not be imported as transactions.
- **MIG-170** [1.0][R] Keep original import files in a protected
  import archive alongside the database for audit purposes.
- **MIG-200** [Later][S] OFX/QFX/CSV import of new transactions from
  institutions, with duplicate detection and matching (shares the
  staging area of MIG-040).

### 12. Reports and Dashboard (RPT, DSH)

#### 12.1 General report features

- **RPT-010** [1.0][S] Reports offer both tables and graphs where
  meaningful.
- **RPT-020** [1.0][S] Report settings (date range, accounts,
  categories, tags, grouping, columns) can be saved as named reports
  and rerun. Every report has one Customize dialog: date range, a
  Display tab (title, grouping, show options, columns with Reset
  Columns), and a tab per filter (Accounts, Categories, Payees,
  Securities, Tags as the report uses them), each with Select All and
  Clear All. Closing a report whose settings changed since it was
  opened or last saved asks whether to save it.
- **RPT-030** [1.0][S] Every number in a report can be drilled into to
  show the contributing transactions (traceability principle).
- **RPT-040** [1.0][R] Date range presets: this month, last month,
  YTD, last year, last 12 months, custom; plus comparison to a prior
  period.
- **RPT-050** [1.0][S] Export to CSV and PDF; print. CSV goes to the
  Downloads folder, every group expanded; printing and PDF show only
  the report. Save PDF… asks Portrait or Landscape (remembered while
  the report is open), saves the PDF to the Downloads folder, and
  opens it in the PDF viewer, which is the preview and prints to
  paper. Kansha has no print-to-paper button: WebKitGTK prints blank
  pages in landscape through its print dialog (even with the
  orientation preset) and ignores CSS `@page` size, so Kansha sets the
  orientation itself and prints only to a file. Save PDF works on
  Linux only for now. On screen and on paper a report is a white page
  with dark text in every theme; its table's heading row stays in view
  while the report scrolls and repeats on each printed page. A report
  with a graph can hide the graph or the table. Printing leaves the
  graph out and sets the report in 9 pt (title 12 pt), whatever the
  screen font size.

#### 12.2 Reports in 1.0

- **RPT-100** [1.0][S] **Spending/Income by category** — for a period,
  with subcategory rollup; optionally by period (columns: week, two
  weeks, half month, month, quarter, half year, year) to show
  trends. The same report **by payee** totals each payee.
- **RPT-110** [1.0][S] **Net worth** — as of a date and over time
  (graph), by account group.
- **RPT-120** [1.0][S] **Account balances/status** — all or selected
  accounts as of a date.
- **RPT-130** **[Withdrawn]** (0.3.22) Tithing report. Existing
  reports (Itemized Categories, Income/Expense by Category) cover it.
- **RPT-140** [1.0][S] **Tax summary** — totals of tax-related
  categories, investment income (dividends, interest, capital gain
  distributions), taxable realized gains (short/long-term), and
  withholdings, for a tax year. This supports tax estimation; it does
  not compute tax (planning is out of scope).
- **RPT-145** [1.0][S] **Tax Schedule** — amounts by tax form and line
  (CAT-050) with their transactions, from taxable accounts; Schedule D
  by holding period from lot disposals. No overall total.
- **RPT-150** [1.0][S] **Realized gains detail** (Capital Gains) —
  lot-level sales for a period, suitable for checking against broker
  Form 1099-B. Subtotal by short vs. long-term, month, quarter, year,
  account, or security, or none.
- **RPT-160** [1.0][R] **Investment income** — by security and
  account, for a period: dividends, interest, short- and long-term
  capital gain distributions, other; cash and reinvested alike.
- **RPT-170** [1.0][R] **Holdings/portfolio value** — positions,
  market value, basis, unrealized gain, as of a date. An account's own
  cash is a row; a holding with no price has no market value.
- **RPT-180** [1.0][S] **Asset allocation** — table and chart, by
  asset class as of a date; investment cash counts as Cash. A class
  opens Holdings for its securities.
- **RPT-190** [1.0][R] **Cash flow** — inflows vs. outflows by month,
  excluding transfers between own accounts.
- **RPT-200** [1.0][R] **Transaction report** — filtered list of
  transactions (general-purpose query tool).
- **RPT-205** [1.0][S] **Itemized Categories** and **Itemized Payees**
  — transactions grouped under INCOME, EXPENSES, and TRANSFERS by
  category (with subcategories) or by payee, with totals. Transactions
  sort by date then account, account then date, amount, or check
  number, ascending or descending.
- **RPT-300** [Later][S] Budgets and budget-vs-actual reports.
- **RPT-310** [1.0][R] Performance reports (TWR/IRR): Investment
  Performance (POS-030).

#### 12.3 Dashboard

- **DSH-010** [1.0][S] Household dashboard showing net worth with
  breakdown (Investments, Cash, Other assets, Liabilities) and this
  month's income, expenses, and net.
- **DSH-020** [1.0][R] Upcoming scheduled transactions (next 14 days,
  configurable) and overdue items.
- **DSH-030** [1.0][R] Warnings panel: missing and stale prices,
  accounts with uncleared transactions more than 60 days old,
  integrity check results, last backup age, date of the last full
  backup verification (BAK-080), backup folder missing (BAK-030).

### 13. Data Integrity, Audit, Backup, and Security (INT, AUD, BAK, SECU)

#### 13.1 Integrity

- **INT-010** [1.0][S] The transaction ledger is the single source of
  truth; balances, positions, and gains are derived.
- **INT-020** [1.0][R] All changes that touch multiple records (e.g.,
  a transfer, a sale consuming several lots) are applied in a single
  database transaction: all or nothing.
- **INT-030** [1.0][R] **Integrity check** (on demand and on a
  schedule, e.g., at startup) verifies invariants:
  - every transaction's postings sum to zero
  - both sides of every transfer exist and match
  - share balances equal the sum of open lot quantities
  - lot basis totals equal acquisitions minus basis consumed by sales
    and adjustments
  - reconciled balances match reconciliation history
  - SQLite `PRAGMA integrity_check` passes
  File > Integrity Check always shows its result in a window. A check
  run automatically shows a window only when it finds problems; a clean
  result appears as a note in the status bar (UI-045).
- **INT-040** [1.0][R] Integrity failures are reported with specific
  records identified; the app never auto-repairs silently.
- **INT-050** [1.0][R] Derived values may be cached for performance,
  but caches are always rebuildable from the ledger and never treated
  as authoritative.

#### 13.2 Audit trail

- **AUD-010** [1.0][S] Every create, edit, void, and delete of
  transactions, lots, accounts, categories, payees, and schedules is
  recorded in an append-only audit log: timestamp, action, entity ID,
  before and after values, and origin (UI, import batch, scheduler).
- **AUD-020** [1.0][R] View audit history for any transaction or
  account from the UI: History… in a register's context menu or an
  investment transaction's dialog, and in the account dialog.
- **AUD-030** [1.0][S] Corrections to historical data are explicit and
  visible, never silent.

#### 13.3 Backup and restore

Decided 2026-09-29 (0.3.30), modeled on LostSheep: the live database
and every backup are encrypted, and one passphrase, the backup
passphrase, unlocks both. No OS keyring is used, since not every
platform has one.

- **BAK-010** [1.0][S] Backups are a first-class feature.
- **BAK-020** [1.0][R] Automatic backup on application close and
  before any import, schema migration, or bulk operation (merge, batch
  rollback); and a timed backup a few minutes after a change (BAK-045).
- **BAK-030** [1.0][R] Manual "Back up now". Every backup, manual or
  automatic, is written to the backup folder chosen in Settings
  (SET-050); with none chosen, to the system Downloads folder. No
  destination is asked for. Setup says that a backup folder on the
  same computer (Downloads included) does not survive the loss of the
  computer; a cloud-synced folder, network drive, or
  USB drive does. Before each backup the folder is checked to be an
  existing folder. If it is not (e.g. after a restore on another
  computer), the backup goes to Downloads and the dashboard warns
  until a folder is chosen; Kansha never creates a file or folder at
  the missing path.
- **BAK-035** [1.0][R] Each backup is one `.zip` file named with its
  date and time; an existing file is never overwritten. It holds:
  - `manifest.json`: backup time (UTC), app version, schema version.
    Nothing financial, since it is readable without the passphrase.
  - the database snapshot, compressed and then encrypted (BAK-060);
  - the private key, locked with the backup passphrase (BAK-060).

  Built (0.3.32, name changed 0.3.37): the file is
  `kansha-<YYYYMMDD>-<HHMMSS>Z-<kind>.zip` (UTC), e.g.
  `kansha-20260929-183012Z-close.zip`; a name already taken gets `-2`,
  `-3`, … Names from before 0.3.37
  (`kansha-backup-2026-09-29T18-30-12Z-close.zip`) are still read and
  pruned by the same rules. The kind is `manual`, `close`,
  `migration`, `bulk` (before a merge), `import`, `restore` (the
  current book, before a restore replaces it), or `timeout` (a timed
  backup, BAK-045). The entries are
  `manifest.json`, `database.gz.age` (gzip, then `age`), and
  `private-key.age`.
- **BAK-040** [1.0][R] Configurable retention (e.g., keep last 10
  automatic backups plus one per month for 12 months). Only automatic
  backups in the backup folder, named as in BAK-035, are ever
  deleted; manual backups and other files never are. A `timeout`
  backup is temporary and outside the count: only the newest one is
  kept, and only while no backup of any other kind (a manual one
  included) is newer.
- **BAK-045** [1.0][S] **Timed backup:** a set number of minutes
  after the first change since the last backup (Settings, default 5;
  0 = off; SET-050), a backup of kind `timeout` is made. The clock runs
  from the first change, not the last, so steady editing still gets a
  backup; a backup of any kind resets it. The status bar says the
  backup is starting and, when it is done, that it finished; a
  failure, a missing folder, or integrity problems flash (UI-045).
  After a failure the next try waits about 5 minutes. Retention keeps
  only the newest timed backup (BAK-040).
- **BAK-050** [1.0][R] Backups are consistent snapshots (SQLite online
  backup API, `VACUUM INTO`, or serialization), never a raw file copy
  of an open database. No unencrypted copy of the database is written
  to disk: the snapshot is taken in memory, or, if that fails under
  SQLCipher, it stays under the database key and that key goes inside
  the encrypted part of the backup.
- **BAK-060** [1.0][R] Backups are encrypted with a public key (`age`,
  X25519). Setting the backup passphrase makes a key pair: the public
  key, which can encrypt but not decrypt and need not be secret, and
  the private key, stored only locked with the passphrase (`age`
  scrypt). Backups therefore need no passphrase, and the passphrase is
  stored nowhere. Every backup carries the locked private key, so a
  restore needs only the backup file and the passphrase.
- **BAK-070** [1.0][R] Restore, on this or any computer, including at
  first start with no database (SECU-080): pick a backup `.zip`, show
  its manifest, ask for the passphrase it was made with, decrypt, run
  the integrity check, and show the comparison window (BAK-075).
  Restore backs up the current database first (when there is one),
  then gives the restored database a new database key (SECU-010) and
  opens it. A backup from a newer schema is refused (§21); one from an
  older schema is migrated on opening.
- **BAK-075** [1.0][R] Restore comparison window, shown before
  anything is overwritten, so the user sees whether the restore would
  replace newer data:
  - backup date and time, and the time of the last change in each
    database (from the audit log);
  - one row per account: number of transactions, backup and current;
    final balance (all transactions, future-dated included), backup
    and current; for an investment account the market value (cash
    plus holdings at each database's latest prices) instead;
  - differences marked with a symbol and bold, not by color; accounts
    only in the backup or only in the current database listed
    separately; a filter to show only rows that differ;
  - Restore and Cancel.
- **BAK-080** [1.0][R] Each backup is checked: the snapshot passes the
  integrity check before it is encrypted; after writing, the zip is
  read back and its manifest, structure, and checksum checked. A full
  decrypt-and-check needs the passphrase: "Verify backup…" and the
  restore drill (§20.3). The dashboard shows the last backup's age and
  the date of the last full verification (DSH-030). A snapshot with
  integrity problems is still backed up (a backup of a damaged book
  beats none); the problems are counted and the dashboard warns
  (INT-040). A backup before a merge or import that fails stops the
  merge or import.

#### 13.4 Security

- **SECU-010** [1.0][R] The database is encrypted at rest with
  **SQLCipher** under a random 256-bit key made when the database is
  created. The key is kept in a key file next to the database,
  encrypted with the backup public key (BAK-060). No OS keyring (D-20).
  Built (0.3.32): `kansha.key` beside `kansha.db`, JSON holding the
  public key, the locked private key, and the database key encrypted
  to the public key. The database opens with the key in SQLCipher's
  raw form (no key derivation).
- **SECU-020** [1.0][R] At startup the user types the backup
  passphrase; it unlocks the private key, which unlocks the database
  key. One passphrase for the database and all backups. A wrong
  passphrase is asked again. "Show database key" in Settings asks for
  the passphrase, then shows the key, which opens the database in DB
  Browser for SQLite (SQLCipher build).
- **SECU-030** [1.0][R] Clear warning at setup: a lost passphrase
  makes the database and all backups unrecoverable. The user is
  prompted to record it durably (e.g., password manager).
- **SECU-040** [1.0][R] Change backup passphrase: asks for the old and
  new passphrase, makes a new key pair, and re-encrypts the key file
  with the new public key; the database key does not change. Old
  backups keep the passphrase they were made with.
- **SECU-050** [1.0][R] External browsing is supported
  read-only. Direct edits via external tools are unsupported; the
  integrity check (INT-030) will detect resulting inconsistencies.
- **SECU-060** [Withdrawn] Auto-lock after an idle period (withdrawn
  0.3.30; the desktop's screen lock serves). May return later.
- **SECU-070** [1.0][R] No network access except explicitly enabled
  features: price download (PRC-040), off until "Allow price download"
  is turned on in Settings. Requests go from Rust, never from the
  webview, and carry only the ticker. No telemetry.
- **SECU-080** [1.0][R] First-run setup, one screen in order: (1)
  create a new database or restore from a backup (BAK-070); (2) choose
  the backup folder (default Downloads, with BAK-030's note); (3) set
  the backup passphrase, with SECU-030's warning. An existing
  unencrypted database goes through the same screens once and is
  converted; the unencrypted file is then deleted.
- **SECU-090** [1.0][R] A missing or damaged key file cannot be
  repaired; the database is recovered by restoring a backup. Changes
  since the last backup are lost; the backup on close (BAK-020) keeps
  that window small.

### 14. User Interface and Settings (UI, SET)

#### 14.1 Navigation and layout

- **UI-010** [1.0][S] One "Accounts" panel beside the register,
  left or right (Settings), with the grouped account list and "Show
  closed accounts". Closed, only its button stays, and clicking it
  drops the list down to pick an account ("Keep open" brings the panel
  back). Open or closed is remembered.
- **UI-020** [1.0][S] Navigation bar under the menu bar with
  user-configurable buttons (Edit > Navigation Bar): Home, the
  Investments screen, any menu item, or any account, in the user's
  order, each with an icon and a text label; Reminders shows the
  number due. The search box (UI-070) is at its right.
- **UI-030** [1.0][S] Account-centric design: each account opens to
  its own view with tabs appropriate to its type (banking: Register |
  Scheduled | Reconcile history; investment: see POS-040).
- **UI-040** [1.0][R] Several reports can be open at once, each in a
  window that fills the view area. Windows not on top wait in a dock
  bar at the bottom of the main window, one text label each; clicking
  a label brings that window to the top. Going to any other view
  leaves the open windows in the dock. The dock is generic. The
  Calendar, Reminders (Scheduled Transactions), Accounts, and
  Reconcile screens are windows too, one each. File > Exit and the
  window's close box ask to save each changed report first; Cancel
  keeps the app open.
- **UI-045** [1.0][R] A status bar shares the row of the Accounts
  button, on the side away from it (left-aligned against the window
  edge when the button is on the right, right-aligned with a margin
  when it is on the left). It stays when the book is empty, without the
  button. Any part of the app may post a note; it clears itself after
  30 seconds. An alert (a backup that failed, was made with a missing
  folder, or found problems; an integrity check that could not run)
  flashes once a second in bold and stays 60 seconds; with reduced
  motion asked for, it does not flash. A new message replaces the
  old one. Back Up Now, report exports (CSV, PDF), and the automatic
  integrity check report there rather than in windows.
- **UI-047** [1.0][R] Help > About Kansha shows the version (for now
  only that).
- **UI-050** [1.0][R] Global keyboard shortcuts for common actions;
  full keyboard operation of the register.
- **UI-060** [1.0][R] Undo for the most recent register change in the
  current session: creating, editing, voiding, or deleting a
  transaction, or changing its cleared status, in banking and
  investment registers. One level; Edit > Undo or Ctrl+Z (outside a
  text field, where Ctrl+Z undoes typing). The transaction comes back
  exactly as it was (same ID, postings, links, and lots), as an
  explicit reversing change recorded in the audit log. Offered only
  while nothing else has changed since; lost when the book closes.
  Undoing a change to a reconciled transaction asks first. Deleting a
  transaction entered from a schedule cannot be undone (it gives the
  occurrence back, REC-160).
- **UI-070** [1.0][R] Search box in the navigation bar: finds
  transactions by payee, category, memo, note, check number, account
  name, or amount, across all accounts or (from a register) in that
  account only. Results are a list, newest first, one line each;
  choosing one opens its account on that transaction.

#### 14.2 Settings

- **SET-010** [1.0][S] Themes: Light, Dark, Classic (the Quicken
  2013 look), Matrix (green on black; OK is cyan and a problem amber,
  since text is green), and Nordic. Nordic is the theme when none is
  stored, whatever the OS prefers. Each theme is a readable CSS file of
  variables (colors only); switching is instant. Picked at the right
  end of the menu bar. Stored per computer, not in the book (SET-070).
- **SET-020** [1.0][S] One base font size, 10–24 px in 1 px steps
  (default 13). All text, spacing, and column widths scale with it;
  themes do not set sizes. Picked at the right end of the menu bar,
  after the font. Stored per computer, not in the book (SET-070).
- **SET-025** [1.0][S] Font: System (default), Arial, Verdana, or
  Courier New, whatever the theme. Each is a font stack that falls back
  to the nearest analogue where the named font is missing (System is
  the OS's own UI font); themes do not set fonts. Picked at the right
  end of the menu bar, between the theme and the size. Stored per
  computer, not in the book (SET-070).
- **SET-030** [1.0][R] Date display format: MM/DD/YYYY (default),
  DD/MM/YYYY, or YYYY-MM-DD. Every user-facing date, shown or typed,
  follows it; a four-digit year typed first is always accepted. Logs
  and histories show timestamps as `YYYY-MM-DDTHH:MM:SSZ` (UTC). First
  day of week.
- **SET-040** [1.0][R] Default lot selection method, which a new
  investment account starts with (each account and security then
  keeps its own, LOT-100); stale-price threshold in days (default 7; a
  security's own value wins). (Tithing percentage withdrawn, 0.3.22.)
- **SET-050** [1.0][R] Backup folder (default: the system Downloads
  folder; BAK-030), retention (BAK-040), and the timed backup's delay
  in minutes (BAK-045; default 5, 0 = off). Change backup passphrase
  (SECU-040); Show database key (SECU-020); Verify backup… (BAK-080):
  Settings offers these three in one "Backup tools" list with an Apply
  button. Allow price download (PRC-040, SECU-070; default off).
- **SET-060** [1.0][R] Startup behavior: "On startup open to:" the
  dashboard, Investments, Reminders, Calendar, Accounts, or any
  account (every new view or account joins the list); run integrity
  check at startup. The Home button always opens the dashboard.
- **SET-070** [1.0][R] Settings are stored in the book's database
  (`setting` table; portable with the data and restored with it),
  except per-computer ones: theme, font, font size (the passphrase
  screen needs them before a book is open), window geometry, and the
  recent books list with their paths. Those are kept in a config file
  in the OS configuration folder, written by Rust. Browser storage
  (localStorage) is not used for settings.

---

## Part III — Non-Functional Requirements (NFR)

- **NFR-010** [1.0][S] Platforms: Kubuntu 26.04 LTS and Windows
  10/11. macOS must not be precluded (no platform-specific code
  outside the native layer).
- **NFR-020** [1.0][S] Single-user, local-only; data in a single
  SQLite database file.
- **NFR-030** [1.0][R] Monetary values are exact: no binary
  floating-point arithmetic anywhere in financial calculations (see
  Section 19).
- **NFR-040** [1.0][R] Performance: register with 10,000 transactions
  opens in under 1 second; typical reports render in under 2 seconds
  on Stan's hardware.
- **NFR-050** [1.0][R] Data volume: comfortably supports 20+ years of
  data (hundreds of thousands of transactions).
- **NFR-060** [1.0][R] Durability: SQLite WAL mode with
  `synchronous=FULL`; no data loss on application crash.
- **NFR-070** [1.0][R] Schema is documented and stable enough for
  external read-only inspection.
- **NFR-080** [1.0][R] Accessibility: keyboard navigation throughout;
  respects font-size setting; adequate contrast in all themes. The
  focused field or button takes the theme's focus background and text
  colors, not only an outline; a text field selects its contents on
  focus so typing replaces them. Applies app-wide.
- **NFR-090** [1.0][R] Dates are calendar dates without time zones
  (financial dates never shift due to time zone conversion).

---

## Part IV — Design

### 15. Design Principles and Rationale

These principles are drawn from the original notes and
discussion. They guide design decisions and resolve conflicts between
requirements.

#### 15.1 The ledger is the source of truth
Balances are never stored as authoritative facts. The chain is:

```
Account → Transactions (postings) → Calculated balance
Investment Account → Security transactions → Lots → Positions → Market value / gain
```

**Rationale:** this makes it possible to reconstruct the state of any
account at any historical date, and to answer "why does this account
say $842,173.19?" by drilling down to the exact transactions. A stored
balance can drift; a derived one cannot.

#### 15.2 Double-entry internally, simple on the surface
Every transaction is stored as postings that sum to zero. Categories
are modeled internally as income/expense ledger accounts, so a grocery
purchase is a posting of −$184.32 to Checking and +$184.32 to the
Groceries category.

**Rationale:** double-entry guarantees money cannot be created or lost
by a bug or a half-finished edit, and makes transfers and splits
natural. The user sees a Quicken-style register, not debits and
credits. Note that this is more rigorous than Quicken itself, which is
essentially single-entry with linked transfers.

#### 15.3 Explicit corrections, never silent changes
Edits are logged with before/after values; destructive actions prefer
void/close over delete; reconciliation adjustments require
confirmation.

**Rationale:** trustworthiness depends on being able to see what
changed and when.

#### 15.4 Start with the essential accounting engine
Tax lots with FIFO and specific identification come first; wash sales,
inherited shares, and complex corporate actions are deferred.

**Rationale:** cost basis is the most complex part of the
model. Getting the core right and tested before adding edge cases
reduces risk.

#### 15.5 Account-centric, not screen-centric
Users navigate to an account and see everything about it (register,
holdings, lots, income, performance) rather than switching between
global screens.

#### 15.6 Modular with defined interfaces
Each module exposes a defined API and does not reach into other
modules' storage directly. This allows later modules (budgets, OFX
import, tax planning) without restructuring.

#### 15.7 Verify against authoritative sources
Seed investment lots from brokerage cost-basis reports rather than
from Quicken; verify imported history against Quicken's own reports;
run Kansha in parallel with Quicken until results agree.

### 16. Technology Stack

#### 16.1 Decided stack [S]

| Layer | Technology |
|---|---|
| UI | Svelte 5 components (HTML, CSS, TypeScript) |
| Frontend build | Vite (plain Svelte; SvelteKit not used) |
| Desktop framework | Tauri v2 |
| Application services and domain engine | Rust (`kansha-core` crate) |
| Numeric | Integer minor units; `rust_decimal` for intermediate math |
| Database | SQLite via `rusqlite` with `bundled-sqlcipher-vendored-openssl` (SQLCipher with its own OpenSSL; no system crypto library needed; encryption per D-20/D-110) |
| Testing | `cargo test`, `proptest`, `insta`, Vitest, Svelte Testing Library |
| Source control / CI | Git + GitHub; GitHub Actions (workflow in the repository, disabled since 2026-09-24 until Stan turns it back on) |

#### 16.2 Decision record

**DR-01 — Accounting engine in Rust (D-30, decided 2026-09-23).**  All
financial logic (ledger, postings, lots, cost basis, recurrence,
reconciliation, reports, integrity checks) lives in Rust. TypeScript
owns presentation only: UI state, forms, display formatting, and
charts.  Rationale:
- Exact arithmetic is enforced by types (integer cents, scaled
  quantities) rather than by discipline in a language whose default
  number type is binary floating point.
- Validation and persistence share one transactional boundary; a
  multi-record change is validated and committed atomically in one
  place, so a UI bug cannot write inconsistent data.
- The engine is testable with `cargo test` independent of the UI.
- Cost accepted: a larger IPC command surface, mitigated by generated
  TypeScript types (D-120).

**DR-02 — Svelte 5 + Vite for the UI (D-35, decided 2026-09-23).**
Rationale:
- The UI is form- and grid-heavy (register, split editor, calendar,
  tabbed account views, modals, reports); a framework avoids large
  amounts of hand-written DOM update code.
- Svelte compiles to small, fast output with minimal boilerplate, and
  Stan already uses it (Photyx), so there is no learning cost.
- Plain Svelte + Vite rather than SvelteKit: SvelteKit's routing and
  server features target web apps and add configuration without
  benefit in a desktop app. Navigation is handled by a simple view
  store.
- The register grid is custom-built; off-the-shelf grids don't fit
  Quicken-style keyboard entry and split expansion.
- Alternatives considered: React (largest ecosystem; more verbose,
  more re-render tuning needed), Vue (reasonable middle ground), Solid
  (small ecosystem), plain TypeScript (most code to maintain).
- Frontend state uses Svelte 5 runes in `.svelte.ts` modules; no
  external state-management library.

**DR-03 — Rust→TypeScript type generation: `tauri-specta` (D-120,
decided 2026-09-24).**  `tauri-specta` + `specta` +
`specta-typescript`, pinned to the exact release candidate
`2.0.0-rc.25` (the newest versions compatible with Tauri v2 at the
time of writing — `tauri-specta`'s last version compatible with Tauri
v1 pulls in a conflicting `gtk-sys`, so it cannot be used with Tauri
v2 at all). Verified in Phase 0: a command decorated with
`#[specta::specta]`, registered through `tauri_specta::Builder`,
compiles against Tauri v2 and generates a matching
`commands.appVersion()` wrapper in `src/lib/types/bindings.ts`
(including its doc comment) on every debug build.  Rationale:
- Generates exactly the shape §17.3 calls for: a typed `commands`
  object, so a mistyped command name or a mismatched argument type is
  a compile error in the frontend, not a runtime IPC failure.
- One source of truth: the command list passed to
  `tauri_specta::Builder` is also what Tauri registers as its invoke
  handler, so the two can't drift apart.
- `ts-rs` (the alternative) only derives per-struct TypeScript types;
  it does not generate command wrappers, so `src/lib/api/` would still
  be hand-written and could still drift from the Rust signatures.
- Risk accepted: both `tauri-specta` and `specta` are pre-1.0 (release
  candidates, `rc.25` at the time of writing) and their API has
  changed across `rc` versions before. The version is pinned exactly
  (`=2.0.0-rc.25`) rather than with a caret range; bumping it is a
  deliberate, tested change, not an automatic `cargo update`.
- `bindings.ts` is generated but checked into the repository (§17.3,
  CONVENTIONS.md) so frontend-only tooling (`svelte-check`, `vite
  build`, CI's frontend job) doesn't need the Rust toolchain. It's
  regenerated with `just bindings` whenever a command signature
  changes.

#### 16.3 Remaining recommendations [R]

**R3 — Database access and encryption.**
- `rusqlite` with `bundled-sqlcipher` for one consistent
  SQLite/SQLCipher version on all platforms. An unkeyed database under
  this build behaves as plain SQLite, so encryption can be turned on
  later without changing libraries (see D-110).
- Numbered, forward-only SQL migrations with a `schema_version` table,
  run by the core crate.
- External browsing via DB Browser for SQLite (SQLCipher-enabled
  build), read-only.

**R4 — Supporting libraries (candidates).**
- Decimal math: `rust_decimal`. Errors: `thiserror`. Serialization:
  `serde`.
- Dates: `time` or `chrono` date-only types in Rust; ISO `YYYY-MM-DD`
  strings in TypeScript. The JavaScript `Date` object is not used for
  financial dates.
- Charts: hand-drawn SVG, no library (D-140, decided Phase 7).
- PDF export: the webview prints the page; no PDF crate. On Linux,
  WebKitGTK's print operation with the page setup set in code
  (`webkit2gtk` and `gtk` crates, the versions Tauri already uses),
  RPT-050.
- Encryption of backups and the key file: `age` crate (X25519
  recipients, scrypt-locked private key). Backup files: `zip` crate.
  No OS keyring (SECU-010). Both pinned (`=0.12.1`, `=7.2.0`, the
  newest for MSRV 1.85). The database key comes from `getrandom`;
  the snapshot is gzipped with `flate2` (both already under `age` and
  `zip`).
- Folder and file pickers (backup folder, restore):
  `tauri-plugin-dialog`, called from Rust only, so the webview gets
  no dialog permission.
- Price download (PRC-040): `ureq` (blocking HTTPS with `rustls`),
  pinned `=2.12.1` (MSRV 1.71), in `src-tauri` only. The reply is
  read in `kansha-core` with `serde_json`'s `raw_value` feature, so
  numbers keep their written text and never pass through a float.
- Testing: see Section 20.

**R5 — Tauri security configuration.**  Tauri v2 capabilities expose
only Kansha's own commands to the frontend; no remote content. The
webview has no network permission: price download (PRC-040) runs in
Rust, and only when enabled (SECU-070). The window's own file
drag-and-drop is off (`dragDropEnabled: false`) so a dropped file
reaches the page (PRC-030).

### 17. Architecture

#### 17.1 Layers

```
┌───────────────────────────────────────────────┐
│ UI (Svelte 5 + TypeScript)                    │
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
| `ledger` | Transactions, postings, splits, transfers, voids, balances, search | accounts, categories |
| `categories` | Categories, payees, tags, memorized payees, tax lines | persistence |
| `schedule` | Scheduled transactions, recurrence rules, occurrence generation | ledger |
| `reconcile` | Reconciliation sessions and history | ledger |
| `securities` | Security master, prices, price list import, price download | persistence |
| `invest` | Investment transactions, lots, cost basis, positions, returns, lot seeding | ledger, securities |
| `reports` | Report definitions, queries, saved reports, dashboard | ledger, invest |
| `import` | Staging, parsers (QIF, CSV, later QXF/OFX), mapping, commit, rollback (Phase 9; not built) | ledger, invest, categories |
| `integrity` | Invariant checks | all read-only |
| `audit` | Append-only change log, per-field history | persistence |
| `undo` | Undo of the last register change (UI-060) | ledger, invest |
| `backup` | Snapshot, retention, verify, restore, timed backup | persistence |
| `book`, `security` | Book files, setup, unlock; key file, passphrase, database key | persistence, backup |
| `settings`, `local_config` | Book settings; per-computer settings | persistence |

Rule: modules interact only through their public APIs; only
`persistence` issues SQL against another module's tables.

#### 17.3 Frontend structure [R]

- `src/lib/api/` — typed wrappers around Tauri commands; the only
  place `invoke` is called. Makes IPC mockable in tests.
- `src/lib/types/` — types generated from Rust (D-120: `tauri-specta`,
  DR-03); not hand-edited. `bindings.ts` is committed and regenerated
  with `just bindings`.
- `src/lib/format/` — the only place money, quantities, and dates are
  formatted or parsed for display. Amounts arrive from Rust as
  integers or decimal strings and are never converted through floating
  point.
- `src/lib/state/` — Svelte 5 rune-based state modules (current view,
  open windows, settings).
- `src/lib/components/` — reusable components (register grid, split
  editor, money input, date input, modal, account picker).
- `src/views/` — top-level views (Dashboard, Account, Accounts,
  Investments, Scheduled, Calendar, Reconcile, Manage, Search, the
  start screen). Reports open in windows
  (`src/lib/components/reports/`); Settings is a dialog.

#### 17.4 Repository layout [R]

```
kansha/
├── devdocs/
│   ├── kansha-spec.md
│   ├── CONVENTIONS.md
│   └── phase-notes/
├── Cargo.toml                 # workspace (kansha-core, src-tauri)
├── crates/
│   └── kansha-core/           # no Tauri dependency
│       ├── src/
│       │   ├── lib.rs
│       │   ├── money.rs       # Money, Quantity, Price newtypes
│       │   ├── date.rs        # date type, Clock trait
│       │   ├── error.rs
│       │   ├── persistence/   # Db, migrate, audit, repositories, migrations/*.sql
│       │   ├── accounts/  ledger/  categories/  schedule/
│       │   ├── reconcile/  securities/  invest/
│       │   ├── reports/  integrity/  audit/  backup/  settings/
│       │   ├── undo.rs        # undo of the last register change (UI-060)
│       │   ├── book.rs        # book files: setup, unlock, restore install
│       │   ├── security.rs    # key file, passphrase, database key
│       │   └── local_config.rs # per-computer config file (SET-070)
│       └── tests/
│           ├── scenarios.rs   # scenario runner
│           ├── properties.rs  # proptest invariants
│           └── integration/   # main.rs + fixture, migrations, schema, repositories
├── tests/
│   └── scenarios/             # TOML scenario files by area
│       └── harness/  ledger/  schedule/  reconcile/  invest/
├── src-tauri/                 # Tauri shell: command handlers + specta builder only
│   ├── src/lib.rs             # run(), specta_builder()
│   ├── src/commands/          # #[tauri::command] handlers, one file per area
│   ├── capabilities/
│   └── icons/
├── src/                       # Svelte frontend (see 17.3)
├── package.json
├── vite.config.ts
├── justfile                   # `just test`, `just dev`, `just check`, `just bindings`
└── .github/workflows/ci.yml   # disabled on GitHub (TEST-140)
```

### 18. Data Model

The schema is defined in SQL, not in this document:
`crates/kansha-core/src/persistence/migrations/0001_init.sql` (plus
any later migrations). Comments in that file explain every table,
column, and constraint. Migrations are forward-only and never edited
once released.

Modeling choices that affect other sections:

- **`txn`**, not `transaction` (SQL keyword).
- **Every table is STRICT.** Money is INTEGER cents; quantity, price,
  and interest rate are INTEGER × 10^6. Dates are TEXT checked with `x
  IS date(x)`. Timestamps are UTC TEXT from the injected `Clock`.
- **Posting sign:** + increases an asset or records an expense; −
  increases a liability or records income. Postings of a transaction
  sum to zero (view `unbalanced_txn`).
- **Register entry view (Phase 2):** the engine stores postings; the
  register reads and writes a transaction as an *entry* seen from one
  account: that account's posting (`amount`) plus lines for the other
  side (categories and transfer accounts). Lines carry the same sign
  as `amount` and must add up to it (TXN-020); the engine computes the
  unassigned remainder. A non-zero entry needs at least one line (no
  uncategorized postings). Transaction-level tags (TAG-010) are stored
  on the viewing account's posting.
- **Void (TXN-040):** the transaction and its postings stay; every
  amount becomes zero and status is `void`. Original amounts live in
  the audit entry. A void can be deleted but not edited. Un-void is
  not offered and will not be (Stan, 2026-09-29): re-enter the
  transaction instead. Undo (UI-060) right after voiding puts it back,
  as it does any last register change.
- **Reconciled edits (TXN-050):** edit, void, delete, or un-reconcile
  of a transaction with a reconciled posting requires explicit
  confirmation. Only reconciliation (or an import, MIG-090) sets
  `reconciled`; an edit keeps an existing reconciled posting's
  reconciliation link.
- **Closed accounts (ACCT-210):** closing requires no transactions
  after the closing date and a zero balance or confirmation. A closed
  account takes no new, edited, voided, deleted, or re-cleared
  transactions until reopened.
- **IPC conventions (Phase 3a):** types that cross IPC derive
  `specta::Type` behind kansha-core's `specta` feature (enabled only
  by src-tauri). Money, quantities, prices, rates, dates, and
  timestamps are canonical strings; enums are snake_case strings;
  tagged unions carry `kind` (`{kind:"category", id}`). IDs and
  counters are i64 in Rust and `number` in TypeScript
  (`dangerously_cast_bigints_to_number`; safe because nothing near
  2^53 is an i64 on the wire and money is never an integer on the
  wire). Commands return `Result<T, IpcError>`; `IpcError.kind` is
  `confirmation_required` when the UI must ask the user and repeat
  with `confirmed = true`. `bindings.ts` is checked by a test
  (`bindings_are_up_to_date`) and regenerated with `just bindings`.
- **Register query (REG-040):** one SQL query numbers every posting of
  the account with a running balance in date order, then filters,
  sorts, and pages, so the balance never depends on filters or
  sort. Category filter includes subcategories and any split line.
- **Payee memorization (PAY-020):** a payee with no defaults learns
  them from its first saved entry; existing defaults are never
  overwritten silently (AUD-030). New payee names are created in the
  same transaction as the entry.
- **Audit view (AUD-020):** the `audit` module turns before/after JSON
  into per-field changes for display.
- **Merges in transaction history (AUD-020, AUD-030):** merging a
  category, payee, or tag also records a `merge` entry on every
  transaction it changed, with that transaction's before and after, in
  the same database transaction. The merged record keeps its own
  `merge` entry.
- **Investment accounts** take no postings through the general ledger
  API; their transactions come from the investments engine (Phase 6).
- **Investment holdings in the ledger:** a posting to an investment
  account with `security_id` carries that holding's cost basis; one
  without is the account's cash. So a buy, sell, or reinvestment
  balances like any other transaction.
- **Equity** category kind, system-only, for opening
  balances. Built-in categories (CAT-060, RCN-040) are seeded by
  migration 0001 and identified by `system_key`.
- **Lots** store immutable acquisition facts. Open quantity and basis
  are derived from `lot_disposal` (sales, transfers out, removals) and
  `lot_adjustment` (splits, return of capital). A partial sale is a
  disposal, not a physical lot split.
- **Schedules** store a template (`schedule` + `schedule_line`) and a
  recurrence rule. Occurrences are stored only once acted on (entered,
  skipped, or edited individually).
- **Schedule rules (Phase 4a):** the recurrence engine generates
  *nominal* dates from the rule alone; the weekend rule (REC-050)
  shifts the due date but the nominal date identifies the
  occurrence. `schedule.next_due` holds the next nominal
  date. Occurrences are handled in order: only `next_due` can be
  entered or skipped. Entering and skipping both use up one of "#
  left" (REC-030). Editing a schedule is "this and all future"
  (REC-120): entered and skipped history stays, the series continues
  after the last occurrence acted on, and pending one-time overrides
  are dropped. "This occurrence only" is a pending
  `schedule_occurrence` row with `override_date` and/or
  `override_amount` (amount on single-line schedules only). Estimated
  amounts need confirmation on entry and are never auto-entered
  (REC-060). Auto-enter (REC-070) enters every due occurrence, missed
  ones included, with origin `scheduler`, and flags each for review
  (`needs_review`, migration 0002) until dismissed. A schedule with
  entered or skipped occurrences is soft-deleted (`status = deleted`)
  so REC-160 links survive; an unused one is removed. The projected
  balance (CAL-050) counts pending occurrences on their due dates,
  overdue ones on today.
- **Reconciliation rules (Phase 5):** a session's check marks are the
  postings' own `cleared` status, so save and resume (RCN-050) need
  nothing beyond the `reconciliation` row. Checking an item marks its
  posting `cleared`; Finish turns every cleared posting dated on or
  before the statement into `reconciled`, linked to the session, and
  needs a zero difference: `statement balance − (Σ reconciled
  postings + Σ checked postings ≤ statement date)`. Reconciliation
  takes and shows every amount in statement sign, as the statement
  prints it: for a credit card (any liability) the balance owed is
  positive, a charge positive, a payment negative. The engine converts
  at its boundary; storage and the register stay in ledger sign. Items
  dated after the statement are never listed or reconciled, even if
  marked cleared in the register. One session in progress per account;
  a new statement date may not precede the last finished
  one. Checking, savings, cash, money market, and credit card accounts
  reconcile (RCN-010), and (Phase 6) investment accounts that keep
  their own cash. The opening balance is the last finished statement's
  ending balance (Σ reconciled postings if none). If reconciled
  postings no longer add up to it, the session still runs and the
  change list (RCN-030) comes from the audit log: transactions whose
  reconciled amount on the account differs from what it was when the
  last statement finished (an edit that leaves the amount alone is not
  listed). Interest earned and a service charge (RCN-020) are created
  with the session as cleared transactions with source `reconcile`; on
  a liability, interest is a charge. A Balance Adjustment (RCN-040) is
  one cleared transaction for the current difference, dated the
  statement date, in the built-in Balance Adjustment category, created
  only with confirmation. Abandoning keeps the session as history and
  leaves check marks as `cleared`. Finish writes one audit entry on
  the reconciliation, not one per transaction. Integrity check
  `reconciled_balance_mismatch` (INT-030) flags an account whose
  reconciled postings differ from its latest finished statement. No
  schema change.
- **Investment rules (Phase 6):** postings per action: buy (cash
  −cost, holding +cost), sell (cash +net proceeds, holding −basis of
  the lots taken, Realized Gain/Loss −gain), income (cash +, built-in
  income category −), reinvest (holding +, income category −), return
  of capital (cash +, holding −basis reduced, Realized Gain/Loss
  −excess), split (one zero holding posting), share transfer (holding
  − here, + there), shares added or removed (holding ± basis against
  Opening Balance), cash in or out (cash ± against an account or
  category), fee, withholding, and misc (cash ± against the built-in
  or a chosen category). With linked cash (INV-300) every cash posting
  goes to the linked account; cash in and out are refused; cash
  handling cannot change once the account has investment
  transactions. Amounts are entered positive and the action gives the
  sign. Buy amount = shares × price + commission (in the basis); sell
  = shares × price − commission; half-even to cents. Lot selection:
  the sale's own method, else the security's default, else the
  account's. FIFO orders by acquisition date, then entry order;
  transferred lots keep their original date. Specific identification
  names lots and shares. Average cost, HIFO, and minimum tax follow
  LOT-110 and LOT-115. Part of a lot takes basis in proportion,
  rounded half-even; a lot's last shares take the rest. Sale proceeds
  divide among lots by shares, remainder cents to the largest
  fractions (ties to the earlier lot). Long-term once the sale is
  after the acquisition's anniversary (29 Feb: after 28 Feb). A split
  rounds the position's new share count once, then divides it among
  the lots by shares; a split that would leave a lot with no shares is
  refused. Return of capital divides by shares; a lot's basis stops at
  zero and the excess is a realized gain with no lot record and no
  holding period. Only lots created by a transaction dated on or
  before the event count. **Date order:** a holding's disposals and
  adjustments form a history in (date, entry) order. A new or changed
  transaction that affects a holding's lots must come after every such
  event already recorded for it, and one can be changed or deleted
  only while nothing follows it. Memo and settlement date can always
  change. Investment transactions are deleted, never voided. A
  reconciled cash posting keeps its status through an edit that leaves
  it in the same account (with confirmation). In an account that holds
  money market funds as cash, they cannot be bought, sold, or moved as
  securities; one with no price is worth $1.00. Market value uses the
  latest price on or before the date; a price older than the
  stale-price setting (SET-040, default 7 days) is stale. The account
  list shows an investment account's cash plus market value, a holding
  with no price at cost. Closing an investment account with cash or
  open positions needs confirmation. An investment account that keeps its own cash
  reconciles that cash (RCN-010); holdings are never listed; statement
  interest and fees become investment transactions (Interest, Fee, or
  misc income or expense for other categories). Lot seeding (MIG-120)
  is one import batch: each CSV row becomes a Shares Added transaction
  on the seeding date whose lot keeps its original acquisition
  date. Integrity checks (INT-030): `lot_overdrawn`,
  `share_balance_mismatch`, `lot_basis_mismatch`,
  `lot_quantity_mismatch`. The audit entry of an investment
  transaction holds the whole transaction with its lot records. No
  schema change.
- **Report rules (Phase 7):** every report is built in Rust as one
  shape: columns and a tree of rows whose groups carry their totals;
  the table, CSV, and printing read it. Report sign: income and money
  coming in positive, spending negative (a category or transfer amount
  is minus its posting). Void transactions and equity (opening
  balance) postings are left out. A category line belongs to the
  transaction's investment account, else its first account posting,
  whichever the account filter includes. Transfers are listed once
  from each included side. Tax Schedule and Tax Summary take only
  transactions of taxable accounts; a transfer counts when the account
  it moves money out of (or into) has a tax line for that
  direction. Capital Gains with no account filter shows taxable
  accounts. Net worth values investment accounts at market value
  (latest price on or before each date, cost when none); its columns
  are the day before the range, each period end, and the last
  date. Without cents, amounts round half-even to dollars after
  totaling. Saved reports store their settings as JSON; settings added
  later default when an older one loads. Migration 0003 adds
  `tax_line`, `category.tax_line_id`, `account.tax_line_out_id` and
  `tax_line_in_id`, and maps the built-in interest, dividend, and
  capital gain distribution categories.
- **Undo (UI-060, 0.4):** around each register change the engine
  takes every row the transaction owns (header, postings and their
  tags, and for an investment transaction its detail row and the lots,
  disposals, and adjustments it made) before and after. Undo writes
  the "before" rows back with their IDs (the header is updated in
  place, so a schedule occurrence's link holds) and records a `create`,
  `update`, or `delete` audit entry with origin `ui`. It runs only
  while the newest audit entry is still the change's own and the rows
  are still as the change left them; otherwise it is refused. The undo
  lives in the running app, not the book. No schema change.
- **Prices (0.4):** the price list import (PRC-030) and price
  download (PRC-040) store prices with source `csv` and `download`. A
  download's date is the provider's market time in the exchange's time
  zone; its price is rounded half-even to 6 decimals from the reply's
  text.
- **Audit log** is append-only, enforced by triggers.
- **Account type** is fixed at creation.

### 19. Numeric Precision and Rounding [R]

- **Money:** stored as 64-bit integers in cents.
- **Quantities (shares):** stored as scaled integers with 6 decimal
  places (covers mutual fund fractional shares; to be confirmed
  against brokerage precision, D-70).
- **Prices:** scaled integers with 6 decimal places.
- **Computation:** `rust_decimal` for intermediate calculations;
  rounding to cents only at defined points (posting creation, basis
  allocation), using round-half-even unless a specific rule requires
  otherwise.
- **Allocation:** when dividing an amount (basis across lots, return
  of capital), allocate remainders deterministically so parts sum
  exactly to the whole.
- **IPC:** amounts cross to TypeScript as integers or strings, never
  as floating-point numbers.

### 20. Testing Framework and Verification (TEST)

A repeatable, automated test framework is a built-in part of Kansha,
not an add-on. The engine's tests define what "correct" means.

#### 20.1 Requirements

- **TEST-010** [1.0][S] The full automated test suite runs with a
  single command (`just test`), is deterministic, requires no network,
  and does not depend on the current date.
- **TEST-020** [1.0][R] **Clock injection:** the engine never reads
  system time directly; "today" (a date) and "now" (a UTC timestamp,
  for `created_at` and audit entries only) are supplied through a
  `Clock` interface. Tests use a fixed clock.
- **TEST-030** [1.0][R] **Unit tests** for pure logic (recurrence date
  generation, lot allocation, rounding, money arithmetic) live
  alongside the code in each Rust module.
- **TEST-040** [1.0][R] **Integration tests** run against a fresh
  in-memory database with all migrations applied, created by a shared
  fixture helper, so every test starts from a known state.
- **TEST-050** [1.0][S] **Scenario tests:** engine behavior is
  specified in human-readable TOML scenario files under
  `tests/scenarios/<area>/`. A single runner discovers and executes
  every file. Each scenario declares an ID, description, the
  requirement IDs it covers, an as-of date, setup (accounts,
  categories, securities, schedules), a sequence of actions, and
  expected results (balances, lots, realized gains, occurrences,
  report totals). Failures report the scenario file, the step, and the
  mismatched field with expected vs. actual values.
- **TEST-060** [1.0][R] Stan can add or modify scenarios without
  writing Rust.
- **TEST-070** [1.0][R] **Traceability:** every engine requirement in
  the TXN, INT, REC, RCN, INV, LOT, POS, and RPT areas is covered by
  at least one scenario or test that cites its ID. `just trace` lists
  requirement IDs with no covering test.
- **TEST-080** [1.0][R] **Property-based tests** (`proptest`) generate
  random transaction sequences and verify the invariants of INT-030
  (postings sum to zero; shares equal open lots; basis conserved
  through splits, transfers, and return of capital; recurrence never
  skips or duplicates occurrences). Failing seeds are committed as
  regression cases.
- **TEST-090** [1.0][R] **Snapshot tests** (`insta`) record report
  output for fixed datasets; any change fails until explicitly
  reviewed and accepted.
- **TEST-100** [1.0][R] **Migration tests:** each schema migration is
  tested by applying it to a database at the previous version
  containing sample data and verifying data integrity afterward.
- **TEST-105** [1.0][R] **Import tests** use sample QIF files
  (synthetic plus sanitized excerpts of Stan's exports) covering the
  pitfalls in MIG-160.
- **TEST-110** [1.0][R] **Test data builders** (a fluent Rust API for
  creating accounts, transactions, lots; `kansha_core::testkit::Book`,
  Phase 2) and a **synthetic dataset generator** producing a
  realistic, multi-year household dataset from a fixed seed, used for
  prototype use, report snapshots, and performance checks (NFR-040).
- **TEST-120** [1.0][R] **Frontend tests** (Vitest + Svelte Testing
  Library, with Tauri IPC mocked) cover money/date input parsing and
  formatting, register keyboard behavior, and split-remainder
  validation. UI tests are deliberately lighter than engine tests.
- **TEST-130** [1.0][R] **Regression rule:** every engine bug fix
  includes a test or scenario that fails before the fix and passes
  after.
- **TEST-140** [1.0][R] **CI:** GitHub Actions runs `cargo fmt
  --check`, `cargo clippy -D warnings`, the Rust suite,
  `svelte-check`, and the frontend suite on Ubuntu and Windows for
  every push.
- **TEST-150** [1.0][R] Coverage is measured (`cargo-llvm-cov`) and
  reported; no hard threshold during the prototype.
- **TEST-160** [Later][R] End-to-end automation of the real
  application window (e.g., WebdriverIO with `tauri-driver`; note
  `tauri-driver` does not support macOS).

#### 20.2 Scenario file format

All amounts, quantities, and prices are written as strings to avoid
TOML floating-point parsing. The format — setup, actions, and
expectations per area — is defined in `tests/scenarios/README.md`.
The examples below show its shape only; they predate it, and some
field names differ (`tests/scenarios/*/` has working files).

```toml
id = "LOT-FIFO-001"
description = "FIFO sale spanning two lots; long-term gain"
requirements = ["LOT-020", "LOT-040", "LOT-100"]
as_of = "2026-06-30"

[[accounts]]
name = "Brokerage"
type = "brokerage"

[[securities]]
ticker = "VTI"
type = "etf"

[[actions]]
date = "2024-01-02"
type = "transfer_cash_in"
account = "Brokerage"
amount = "50000.00"

[[actions]]
date = "2024-01-10"
type = "buy"
account = "Brokerage"
security = "VTI"
quantity = "100"
price = "200.00"
commission = "0.00"

[[actions]]
date = "2024-06-10"
type = "buy"
account = "Brokerage"
security = "VTI"
quantity = "50"
price = "220.00"
commission = "0.00"

[[actions]]
date = "2026-03-15"
type = "sell"
account = "Brokerage"
security = "VTI"
quantity = "75"
price = "250.00"
commission = "0.00"
method = "fifo"

[[expect.lots]]
account = "Brokerage"
security = "VTI"
acquired = "2024-01-10"
quantity = "25"
basis = "5000.00"

[[expect.lots]]
account = "Brokerage"
security = "VTI"
acquired = "2024-06-10"
quantity = "50"
basis = "11000.00"

[[expect.realized]]
sale_date = "2026-03-15"
acquired = "2024-01-10"
quantity = "75"
proceeds = "18750.00"
basis = "15000.00"
gain = "3750.00"
term = "long"

[expect.cash]
"Brokerage" = "37750.00"
```

A recurrence scenario follows the same pattern:

```toml
id = "REC-MONTHEND-001"
description = "Monthly on the 31st clamps to month end"
requirements = ["REC-020", "REC-040"]
as_of = "2026-01-01"

[[schedules]]
name = "Rent"
start = "2026-01-31"
frequency = { kind = "monthly", day = 31 }
end = { kind = "never" }

[[expect.occurrences]]
schedule = "Rent"
through = "2026-04-30"
dates = ["2026-01-31", "2026-02-28", "2026-03-31", "2026-04-30"]
```

#### 20.3 Verification beyond automated tests

1. **Migration verification** (MIG-100): Kansha balances and category
   totals match Quicken reports exactly.
2. **Parallel run** (D-80): Kansha and Quicken used side by side,
   including at least one full reconciliation cycle per account,
   before Quicken is retired.
3. **Backup/restore drills:** restore a backup to a scratch location
   and run the integrity check.
4. **Stan reviews** the lot, recurrence, and reconciliation scenario
   suites before each phase is closed.

### 21. Versioning, Schema Migration, and Release [R]

- Application follows semantic versioning; 1.0.0 is the first release
  Stan relies on for real data.
- Database schema has its own version number; migrations run
  automatically at startup after an automatic backup (BAK-020), and
  are forward-only.
- The app refuses to open a database with a newer schema version than
  it supports.
- This document is versioned in Git alongside the code; significant
  changes are logged in Appendix A.

---

## Part V — Open Decisions and Placeholders

### Decisions needed

| ID | Decision | Status | Recommendation / Outcome |
|---|---|---|---|
| D-10 | Register: combined signed amount column or separate Payment/Deposit columns | **Decided** (2026-09-24) | Separate Payment and Deposit columns (REG-010). No schema impact; postings stay signed. |
| D-20 | Encryption: SQLCipher vs. disk-level only | **Decided** (2026-09-29) | SQLCipher with a random key in a key file encrypted to the backup public key; one backup passphrase at startup; public-key encrypted backups; no OS keyring (SECU-010, SECU-020, BAK-060) |
| D-30 | Accounting engine in Rust vs. TypeScript | **Decided** | Rust (DR-01) |
| D-35 | UI framework | **Decided** | Svelte 5 + Vite, no SvelteKit (DR-02) |
| D-40 | Price download in 1.0, and which provider | **Decided** (2026-09-29) | In 1.0 behind a provider interface; Yahoo Finance's chart service first (no key), a keyed provider later; latest price only, on demand, off until enabled (PRC-040, SECU-070) |
| D-50 | Money market funds: security or cash | **Decided** | Per-account option (`account.mmf_mode`) |
| D-60 | Lot selection methods | **Decided** | `fifo`, `specific`, `average` (LOT-110, migration 0004), `hifo`, and `min_tax` (LOT-115), all built |
| D-70 | Share/price decimal precision | Open | 6 decimal places; confirm with brokerage data |
| D-80 | Parallel-run duration before retiring Quicken | Open | 2–3 months |
| D-90 | Confirm 1.0 report list (Section 12.2) | Open | As listed |
| D-100 | Confirm account type list, including 401(k) and Loan/Mortgage | **Decided** | As listed in ACCT-010/020 |
| D-110 | Encryption during the prototype | **Decided** | Unencrypted prototype database (synthetic data only) using the same SQLCipher build; encryption enabled in Phase 8 (decided 2026-09-29) |
| D-120 | Rust→TypeScript type generation for IPC | **Decided** | `tauri-specta` + `specta` + `specta-typescript`, pinned to `2.0.0-rc.25` (DR-03) |
| D-130 | Prototype data | Open | Synthetic data only; no real financial data until 1.0 development |
| D-140 | Chart library | **Decided** (2026-09-27) | None: hand-drawn SVG. Rust places values on the axis; the frontend only scales. Series differ by pattern and shape as well as color (red-green colorblind). |

### Placeholders

| ID | Placeholder | Needed to resolve |
|---|---|---|
| P-01 | Source format for income/expense history (QIF vs. QXF) | Export a small account in both formats and inspect them in a text editor |
| P-02 | Quicken reports used as reference for import verification | Choose reports (e.g., account balances as of export date; category totals by year) |
| P-03 | Source for seeding investment lots | Check each brokerage's cost-basis/unrealized-gain CSV export |
| P-04 | Whether Quicken scheduled transactions can be exported | Test export; otherwise plan manual re-entry |
| P-05 | Whether QIF exports from Quicken 2013 include categories, tags, securities, and prices in usable form | Test full QIF export |

---

## Part VI — Prototype Plan

### 22. Purpose and Approach

The prototype validates the requirements and design before 1.0
development, using synthetic data. It implements as many 1.0
requirements as practical, excluding Quicken import (pending
placeholders P-01 to P-05).

**Evolutionary (kept for 1.0):**
- `kansha-core`: schema, migrations, engine, services
- All tests, scenario files, builders, and the dataset generator
- The IPC command API and generated types

**Throwaway-permitted (may be rewritten after review):**
- Svelte views, layouts, and styling

Throwaway code still follows CONVENTIONS.md: it uses `src/lib/api` for
all IPC and never performs money arithmetic.

### 23. Prototype Scope

**Built in the prototype (Phases 0–8):** ACCT, CAT, PAY, TAG, TXN,
REG, REC, CAL, RCN, SEC, PRC (manual, price list import, download),
INV, LOT (all five methods), POS, RPT, DSH, INT, AUD, BAK, SECU, UI,
SET, TEST, and MIG-120's lot seeding on synthetic data. Not built:
MIG (Phase 9) and the [1.0] items listed as missing in
`devdocs/phase-notes/prototype-review.md`, which keep their status
until decided (RPT-040 comparison, RPT-120, RPT-190, RPT-200,
TAG-030 grouping, UI-030 tabs, UI-050, TEST-070, TEST-090, TEST-150,
and TEST-140 while CI is off). [Later] items are out of scope.

### 24. Phases

Each phase ends with its exit criteria met, `just test` passing on
Kubuntu and Windows, and Stan's review. The schema (spec 0.3) is
delivered as migration 0001 in Phase 1.

| Phase | Content | Exit criteria |
|---|---|---|
| **0 — Skeleton and test harness** | Cargo workspace; `kansha-core` crate; Tauri shell; Svelte app shell with view navigation and theme/font-size plumbing; `Money`/`Quantity`/`Price` newtypes; date type and `Clock`; error types; scenario runner with one trivial scenario; `justfile`; CI; decide D-120 | App launches on both platforms; `just test` and CI green; scenario runner reports a deliberately failing scenario clearly |
| **1 — Schema and persistence** | Migrations from spec 0.3 schema; repositories; audit log writer; in-memory test fixture; migration test pattern | Migration and repository tests pass; schema matches spec |
| **2 — Ledger engine** | Accounts, categories, payees, tags, transactions, postings, splits, transfers, voids, derived balances; integrity check v1; test data builders | Ledger scenarios and posting/transfer property tests pass |
| **3 — Register UI** | Account list/sidebar, account modal, register with keyboard entry, splits, transfers, filters, memorized payees, category, tag, and payee management screens, audit view; synthetic dataset generator | Stan enters a month of transactions by keyboard; generator loads a multi-year dataset; register meets NFR-040 |
| **4 — Scheduling and calendar** | Recurrence engine; occurrences; enter/skip/edit-one; due and overdue list; scheduled list; calendar | Recurrence suite passes (month-end, leap years, Nth weekday, twice-monthly, weekend shifting, # left, end dates) |
| **5 — Reconciliation** | Reconcile workflow, save/resume, history, change detection, explicit adjustments | Reconciliation scenarios pass; Stan completes a reconciliation on synthetic data |
| **6 — Investments** | Securities; manual/CSV prices; investment transactions; lots (FIFO, specific ID; later average, HIFO, minimum tax); splits; return of capital; share transfers; positions; the Investments screen (POS-040); lot seeding via CSV (MIG-120 mechanics only, synthetic data) | Lot scenario suite and basis-conservation properties pass; Stan reviews lot scenarios |
| **7 — Reports and dashboard** | 1.0 report list; saved reports; drill-down; CSV export; charts; dashboard | Report snapshot tests pass; drill-down reaches transactions for every figure |
| **8 — Encryption, backup, settings, review** | Database encryption and first-run setup (SECU); encrypted backups, manual and on close; restore with comparison window; verification; settings; performance check; prototype review | Restore drill passes; review findings recorded for spec 0.4 |

### 25. Workflow

Kansha is built with Claude Code in the repository. `CLAUDE.md` holds
the working rules (it replaces the chat workflow of spec 0.2–0.3,
with its zip and patch delivery); `devdocs/CONVENTIONS.md` §3–§8 are
binding. Each phase ends with `devdocs/phase-notes/phase-N.md`: files
created, decisions made, known gaps.

---

## Appendix A — Change Log

| Version | Date | Changes |
|---|---|---|
| 0.4 | 2026-09-29 | Prototype review (`devdocs/phase-notes/prototype-review.md`). Spec brought up to date: header status; ACCT-100 (no icon-bar flag); REG-080 (menu as built, with Split); INV-010 table restored; POS-030 old recommendation removed; DSH-030 (uncleared over 60 days); AUD-020 (account history); BAK-030 (Setup warns, not Settings); SET-050, SET-070 (font); UI-010, UI-020 (Accounts panel, navigation bar); §16.1 CI off; §16.3 R4, R5; §17.2–§17.4 modules and layout; §18 undo and price rules; §20.2; §23 rewritten as what was built; §24 Phase 6; §25 replaced by a pointer to CLAUDE.md; this log in one order, newest first. New or changed and built: **BAK-045** timed backup (split out of SET-050); **PRC-030** price list import (ticker, price, optional `MM/DD/YYYY`; date picker; file picker or drop; replaces the dated CSV import; QIF prices only through MIG-140); **PRC-040** price download, [1.0], D-40 decided (Yahoo first, latest price, off until enabled); **PRC-050** Holdings marks stale prices; **SECU-070** the download setting; **SET-040** default lot method for new investment accounts; **UI-060** undo of the last register change (Edit > Undo, Ctrl+Z). No schema change. **API change:** `price_import_preview`/`price_import` take a `date`; `account_defaults` reads the book (returns a result); `Settings.default_lot_method`, `Settings.price_download`; new commands `undo_status`, `undo_apply`, `prices_download`. |
| 0.3.37 | 2026-09-29 | Backup file names are `kansha-YYYYMMDD-HHMMSSZ-<kind>.zip` (old names still read). BAK-035, BAK-040, SET-050: timed backups, kind `timeout` (5 minutes after the first change, setting `backup_timeout_minutes`, 0 = off; only the newest kept, deleted once any other backup is newer; status bar shows start and finish). UI-047: Help > About shows the version. The app is version 0.7.0. No schema change. **API change:** commands `backup_timed_due`, `backup_timed_run`; `Settings.backup_timeout_minutes`; `BackupKind` gains `timeout`. |
| 0.3.36 | 2026-09-29 | SET-025: Font picker (System, Arial, Verdana, Courier New) beside theme and size; themes no longer set a font; Nordic Courier removed (a stored one becomes Nordic with Courier New). SET-010, SET-020 reworded. No schema change. **API change:** `appearance_get` and `appearance_set` carry a `font` field (per-computer config file, optional). |
| 0.3.35 | 2026-09-29 | SET-010: Nordic theme (from lostsheep; filled navigation bar like Classic; light-blue OK, amber problem) and Nordic Courier (same, Courier New). Nordic is the default theme; the OS light/dark preference no longer picks one. No schema or API change. |
| 0.3.34 | 2026-09-29 | SET-010: Matrix theme (colors from lostsheep, Courier New; cyan OK, amber problem). No schema or API change. |
| 0.3.33 | 2026-09-29 | UI-045: status bar on the Accounts button row (notes clear after 30 s; alerts flash, 60 s). INT-030: an automatic check opens a window only for problems. Back Up Now and report exports report in the status bar. No schema or API change. |
| 0.3.32 | 2026-09-29 | Phase 8 built (encryption, backup, restore, settings). BAK-035: file and entry names, backup kinds. BAK-040: only automatic backups are pruned. BAK-080: a snapshot with integrity problems is still backed up and the dashboard warns; a failed backup before a merge or import stops it. SECU-010: `kansha.key` layout, raw SQLCipher key. §16.3 R4: `age`/`zip` pins, `getrandom`, `flate2`, `tauri-plugin-dialog`. §17.4: `book.rs`, `security.rs`, `local_config.rs`. No schema change. API change: 18 new IPC commands (book, backup, restore, settings, appearance, pickers); `Dashboard.backup`; error kinds `wrong_passphrase`, `locked`. |
| 0.3.31 | 2026-09-29 | SET-070: all settings in the book's `setting` table except per-computer ones (theme, font size, window geometry, recent books), which go to a config file in the OS configuration folder; no localStorage. SET-010, SET-020: stored per computer. BAK-030, DSH-030: a missing backup folder falls back to Downloads with a dashboard warning; no file is created at the missing path. No schema or API change. |
| 0.3.30 | 2026-09-29 | Backup and encryption decided (§13.5 proposal adopted and removed). BAK-030 (every backup to the Settings folder, default Downloads), BAK-035 (zip layout), BAK-050, BAK-060 (public-key encrypted backups, `age`), BAK-070, BAK-075 (restore comparison window), BAK-080 rewritten or added. SECU-010 (random database key in a key file encrypted to the backup public key; no OS keyring), SECU-020 (backup passphrase at startup; Show database key), SECU-040 (change backup passphrase), SECU-080 (first-run setup), SECU-090 added; SECU-060 withdrawn. DSH-030, SET-050, D-20, D-110, §16.3 R4, §23, §24 Phase 8 updated. No schema or API change. |
| 0.3.29 | 2026-09-29 | §13.5 added: proposal for backup and encryption (backup folder in Settings, default Downloads; zipped backups; database key in the OS keyring; backups encrypted with a public key, restored with a backup passphrase), with open questions. To be discussed before a final decision; BAK and SECU stand until then. D-20 under discussion. No schema or API change. |
| 0.3.28 | 2026-09-29 | SET-010, SET-020: the theme and font size pickers move from the Settings dialog to the right end of the menu bar. No schema or API change. |
| 0.3.27 | 2026-09-29 | TXN-040: un-void will not be offered. AUD-020: category, payee, and tag merges record a `merge` entry in the history of each transaction they change. No schema or API change. |
| 0.3.26 | 2026-09-28 | SET-010: Classic theme added; themes are CSS files of variables (`src/css/themes/`), with global element styles in `src/css/base.css`. SET-020: one base font size on `<html>`, 10–24 px in 1 px steps, default 13; named text sizes in rem. No schema or API change. |
| 0.3.25 | 2026-09-28 | REG-050: a split's Tag column shows only its own tag; Split button in the entry row. Calendar: only skipped items are struck through. Scrollbars are classic (they take width) instead of GTK overlay ones, which covered the register's last column. No schema or API change. |
| 0.3.24 | 2026-09-28 | REC-160: deleting a transaction entered from a schedule gives its occurrence back (the latest one returns to Due with "# left" restored; an earlier, auto-entry, or deleted-schedule one becomes skipped); it was refused before. CAL-020, CAL-030: the calendar and its day dialog show register transactions too (not investment accounts or voids). Register scrolls continuously (all rows loaded, only those in view drawn); split lines get a Tag picker. New IPC command `calendar_transactions`. No schema change. |
| 0.3.23 | 2026-09-28 | SET-060: "On startup open to:" setting (was Home screen); the Home button always opens the dashboard. REG-070: future rows italic with their own alternate-row tint, not dimmed; reconciled rows gray, in banking and investment registers. No schema or API change. |
| 0.3.22 | 2026-09-28 | Tithing withdrawn: goal 5 no longer names it; CAT-040 keeps only the tax-related flag; RPT-130 and SET-040's tithing percentage **[Withdrawn]**. Giving is reviewed with existing reports. No schema or API change (the unused `tithable` and `giving` columns stay). |
| 0.3.21 | 2026-09-28 | §17.4: `src-tauri/src/commands/` is a folder, one file per area (was `commands.rs`). No requirement change. |
| 0.3.20 | 2026-09-28 | RPT-050: Save PDF… (orientation, then a PDF in Downloads opened in the PDF viewer) replaces Print…; the system print dialog printed blank pages in landscape. Paper printing is from the PDF viewer. §16.3 R4: PDF through WebKitGTK's print operation on Linux (new Linux-only deps `webkit2gtk`, `gtk`). New IPC command `report_save_pdf`. No schema change. |
| 0.3.19 | 2026-09-28 | RPT-050: printed reports leave out the graph and use 9 pt. |
| 0.3.18 | 2026-09-28 | LOT-110 (average cost; migration 0004 adds lot adjustment kind `average`) and LOT-115 (HIFO, minimum tax) built and moved to 1.0. POS-030 defined and built (IRR and time-weighted return; Investment Performance report); RPT-310 moved to 1.0. RPT-160, RPT-170, RPT-180 built. D-60 fully decided. |
| 0.3.17 | 2026-09-28 | UI-040: the window's close box also asks to save changed reports. No schema or API change. |
| 0.3.16 | 2026-09-28 | UI-040: Calendar, Reminders, Accounts, and Reconcile open as dockable windows; File > Exit asks to save changed reports. CAL-030: day dialog (Enter, Edit, Skip, Close, New Schedule). No schema or API change. |
| 0.3.15 | 2026-09-28 | UI-040: reports open in windows with a dock bar (replaces tabs). RPT-020: closing a changed report asks to save. RPT-050: white report page, frozen table heading, hide graph or table. Reports menu grouped into Investing, Net Worth, Spending, and Tax submenus. No schema or API change. |
| 0.3.14 | 2026-09-27 | Report toolbar: RPT-100 gains Income/Expense by Payee and the full interval list; RPT-150 lists its subtotals; RPT-205 gains sort by check number and descending order. No schema change. |
| 0.3.13 | 2026-09-27 | Phase 7 (reports and dashboard). CAT-050 moved to 1.0 and built (migration 0003: tax lines; category and account transfer mappings). RPT-020 describes the shared Customize dialog. RPT-050 and RPT-150 accepted. RPT-145 (Tax Schedule) and RPT-205 (Itemized Categories and Payees) added. D-140 decided: hand-drawn SVG graphs. §16.3 PDF export through the print dialog. §18 gains report rules. |
| 0.3.12 | 2026-09-26 | POS-040 rewritten: the Investments screen (account, equity, and lot tree with named views, as-of date, and day change) replaces the six account tabs; an investment account opens as a register. Income, Performance, and realized gains wait for the reports. New IPC command `inv_portfolio`. No schema change. |
| 0.3.11 | 2026-09-24 | Phase 6 (investments). §18 gains investment rules (postings per action, lot selection and rounding, splits, return of capital, the date-order rule for a holding's history, linked cash, money market funds, stale prices, account list value, investment cash reconciliation, lot seeding as an import, new integrity checks). §18 reconciliation rules: investment accounts with their own cash reconcile. LOT-115: refused until built, not in Phase 6. No schema change. |
| 0.3.10 | 2026-09-24 | SET-030: three date formats (MM/DD/YYYY default, DD/MM/YYYY, YYYY-MM-DD) for every user-facing date; logs and histories use `YYYY-MM-DDTHH:MM:SSZ`. NFR-080: theme focus colors (background and text) and select-on-focus, app-wide. |
| 0.3.9 | 2026-09-24 | Reconciliation uses statement sign: a credit card's ending balance is entered and shown as the statement prints it (owed = positive); charges positive, payments negative. Ledger sign unchanged elsewhere. |
| 0.3.8 | 2026-09-24 | Phase 5 (reconciliation). §18 gains reconciliation rules (check marks are cleared status, Finish scope, opening balance and change detection from the audit log, statement items, Balance Adjustment, abandon). INT-030 reconciled-balance check implemented. No schema change; 11 IPC commands added. Scenarios under `tests/scenarios/reconcile/`. |
| 0.3.7 | 2026-09-24 | Navigation bar search (UI-070) replaces the register's text-search box; REG-040 no longer lists text search among the register filters. |
| 0.3.6 | 2026-09-24 | Phase 4b. REC-030: skipping an occurrence uses up one of "# left", like entering it (confirmed by Stan; §18 already said so). |
| 0.3.5 | 2026-09-24 | Phase 4a. §18 gains schedule rules (in-order handling, "# left" on skip, nominal vs. due date, one-time overrides, auto-enter review flag, soft delete). Migration 0002 adds `schedule_occurrence.needs_review`. Recurrence scenarios under `tests/scenarios/schedule/`. |
| 0.3.4 | 2026-09-24 | Phase 3 scope (§24) now names the category, tag, and payee management screens. |
| 0.3.3 | 2026-09-24 | D-10 decided: separate Payment and Deposit columns (REG-010). |
| 0.3.2 | 2026-09-24 | Phase 3a. §18 gains IPC conventions, register query, payee memorization, and audit view choices. `sample` module provides the synthetic dataset (TEST-110, D-130); NFR-040 measured (see phase-notes/phase-3.md). |
| 0.3.1 | 2026-09-24 | Phase 2 (ledger engine). §18 gains modeling choices for the register entry view, void, reconciled edits, closed accounts, and investment-account postings. TEST-110 names `testkit::Book`. §20.2 points to `tests/scenarios/README.md` for the implemented scenario format. |
| 0.3 | 2026-09-24 | Phase 1. §18 replaced by a pointer to `migrations/0001_init.sql` (the schema is now defined in SQL) and a short list of modeling choices. D-50, D-60, D-100, D-110 decided; D-10 marked schema-neutral. ACCT-020 and INV-050 accepted ([S]). LOT-115 added (HIFO and minimum-tax lot selection). LOT-110 notes that the schema accepts `average`. TEST-020: `Clock` also supplies UTC timestamps. §16.1: `bundled-sqlcipher-vendored-openssl`. §17.4 layout updated. |
| 0.2.1 | 2026-09-24 | D-120 decided: `tauri-specta`/`specta`/`specta-typescript` pinned to `2.0.0-rc.25` (DR-03), verified against Tauri v2 in Phase 0. §17.3 and §17.4 updated to reflect the built `src-tauri` layout and committed `bindings.ts`. §25 patch command corrected to `git apply` (matches CONVENTIONS.md; the draft had said `patch -p1`). |
| 0.2 | 2026-09-23 | D-30 decided (Rust engine) and D-35 decided (Svelte 5 + Vite); Section 16 rewritten with decision record; layer diagram label updated; Sections 17.3 (frontend structure) and 17.4 (repository layout, `kansha-core` crate, `devdocs/`) added; Section 20 replaced with Testing Framework (TEST-010–160, including TEST-105 import tests) and scenario format; decisions table gains Status column and D-110–D-140; Part VI Prototype Plan added, including the single-patch convention for changes. |
| 0.1 | 2026-09-23 | Restructured from initial notes; added requirement IDs, release/source tags, recommendations, design rationale, open decisions, and migration placeholders. |
