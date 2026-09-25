# Scenario files

Engine behavior in plain TOML (spec §20, TEST-050, TEST-060). The runner
(`crates/kansha-core/tests/scenarios.rs`) runs every `*.toml` here on a
fresh in-memory database. `just test` runs them all;
`just scenario tests/scenarios/ledger` runs one folder or file.

Rules (CONVENTIONS §7):

- Amounts are **strings**: `"-184.32"`, never `-184.32`.
- Inline tables stay on **one line**. An array of them may span lines.
- Every scenario lists the requirement IDs it covers.
- Unknown fields are errors, so a typo fails loudly.
- After the last action the integrity check (INT-030) runs and must be
  clean.

## Header

```toml
id = "LEDGER-001"                 # unique across all files
description = "What this proves"
requirements = ["TXN-010", "REG-020"]
as_of = "2026-03-15"              # "today" for the engine
```

## Setup

```toml
[[accounts]]
name = "Visa"
type = "credit_card"              # checking, savings, credit_card, cash, money_market,
                                  # other_asset, other_liability, loan, brokerage,
                                  # traditional_ira, roth_ira, hsa, retirement_401k
credit_limit = "5000.00"          # optional, credit cards only
opening_balance = "-500.00"       # optional, ledger sign (negative = owed)
opening_date = "2026-01-01"       # required with opening_balance
tax_treatment = "tax_deferred"    # optional; defaults by type
# Investment accounts only, all optional:
cash_mode = "linked"              # internal (default) or linked
linked_cash = "Checking"          # with linked: a cash account listed earlier
mmf_mode = "security"             # cash (default) or security
lot_method = "specific"           # fifo (default) or specific

[[categories]]
path = "Food:Groceries"           # parents are created too
kind = "expense"                  # expense or income
```

Payees and tags are created the first time an action names them.

```toml
[[securities]]
name = "Vanguard Total Stock Market ETF"
ticker = "VTI"                    # optional; actions name a security by ticker or name
type = "etf"                      # stock, etf, mutual_fund, bond, money_market, cd, other
asset_class = "us_equity"         # optional: us_equity, intl_equity, bond, cash,
                                  # real_estate, commodity, other (default by type)
lot_method = "specific"           # optional: overrides the account's default

[[prices]]
security = "VTI"
date = "2026-06-30"
price = "320"
```

## Signs

Ledger sign, as the account sees it:

- Checking, savings, cash: `-` payment, `+` deposit.
- Credit card, loan: `-` charge (more owed), `+` payment.
- Split lines use the **same sign as the entry amount** and must add up
  to it.

Category totals come out expense `+`, income `-`.

## Actions

Any action may add `expect_error = "text"`: the action must fail with an
error containing that text, and the scenario carries on.

### entry

```toml
[[actions]]
type = "entry"
ref = "costco"                    # optional name for later actions
date = "2026-01-05"
account = "Checking"
amount = "-184.32"
payee = "Costco"                  # optional
check_num = "1001"                # optional
memo = "..."                      # optional
cleared = "cleared"               # optional: unmarked, cleared
tags = ["Trip"]                   # optional
category = "Food:Groceries"       # or: transfer = "Savings"
```

`category` or `transfer` takes the whole amount, or whatever the split
lines leave. Split lines:

```toml
[[actions]]
type = "entry"
date = "2026-02-01"
account = "Checking"
amount = "-300.00"
transfer = "Savings"              # takes the remaining -10.00

[[actions.lines]]
category = "Food:Groceries"
amount = "-250.00"
memo = "food"                     # optional; also tags, cleared (transfer lines)

[[actions.lines]]
category = "Household"
amount = "-40.00"
```

### edit

Same fields as `entry`; `ref` is required and names the transaction to
replace. `account` is the side you edit from (either side of a
transfer). Add `confirm = true` to edit a reconciled transaction.

### void, delete

```toml
[[actions]]
type = "void"                     # or "delete"
ref = "costco"
confirm = false                   # true for a reconciled transaction
```

### set_cleared

```toml
[[actions]]
type = "set_cleared"
ref = "move"
account = "Savings"               # which side
cleared = "cleared"               # unmarked or cleared
```

### Accounts

```toml
[[actions]]
type = "close_account"
account = "Old Savings"
date = "2026-04-30"
confirm = true                    # needed when the balance isn't zero

[[actions]]
type = "reopen_account"           # or "delete_account"
account = "Old Savings"
```

### Merges

```toml
[[actions]]
type = "merge_categories"         # or merge_payees, merge_tags
from = "Supermarket"
into = "Food:Groceries"
```

### Schedules (Phase 4)

```toml
[[actions]]
type = "schedule"
ref = "rent"                      # name for later actions and expectations
account = "Checking"
payee = "Landlord"                # optional
amount = "-1000.00"               # the main account's amount, register sign
category = "Housing"              # or transfer = "Savings"; takes the amount
                                  # left after any [[actions.lines]]
frequency = "monthly"             # once, daily, weekly, twice_monthly, monthly,
                                  # monthly_last_day, monthly_nth_weekday, yearly
interval = 1                      # optional (quarterly = monthly, interval 3)
day1 = 1                          # monthly, twice_monthly; day2 too for twice
weekday = 2                       # monthly_nth_weekday: 1 = Monday ... 7
week_of_month = 2                 # 1-4, or -1 = last
start = "2026-01-01"
weekend_rule = "previous"         # optional: none, previous, next
end_date = "2026-12-31"           # or count = 12 ("# left"); optional
remind_days = 3                   # optional
mode = "auto"                     # optional: remind (default), auto
amount_type = "estimated"         # optional: fixed (default), estimated
tag = "Trip"                      # optional, on the first line
```

`enter_occurrence` (`ref`, `due` = nominal date, optional `date`, `amount`,
`confirm`, `txn_ref` to name the transaction), `skip_occurrence` (`ref`,
`due`), `override_occurrence` (`ref`, `due`, `date` and/or `amount`; neither
clears it), and `auto_enter` (runs auto-entry as of `as_of`; optional
`expect_entered = N`).

Expectations are checked after **all** actions, so an occurrence entered
by an action is no longer pending. Use a second, untouched schedule to check
a full series.

### Reconciliation (Phase 5)

```toml
[[actions]]
type = "reconcile_start"
account = "Checking"
statement_date = "2026-01-31"
statement_balance = "1400.00"     # ledger sign: owed on a credit card = negative
interest = { date = "2026-01-31", amount = "2.50", category = "Interest" }      # optional
service_charge = { date = "2026-01-31", amount = "5.00", category = "Bank Fees" }  # optional
```

The other reconcile actions work on the account's session in progress:

- `reconcile_check`: `account`, `refs = ["a", "b"]` (names given to
  transactions with `ref`), `checked = false` to uncheck.
- `reconcile_update`: `account`, `statement_date` and/or `statement_balance`.
- `reconcile_adjust`: `account`, `confirm = true` (Balance Adjustment).
- `reconcile_finish`, `reconcile_abandon`: `account`.

To keep a transaction reconciled when editing it, give the `edit` action
`cleared = "reconciled"` and `confirm = true`.

Interest and service charge amounts are positive; the engine picks the
sign.

### Investments (Phase 6)

```toml
[[actions]]
type = "invest"
ref = "buy1"                      # optional name for later actions
account = "Brokerage"
action = "buy"                    # buy, sell, dividend, interest, reinvest_dividend,
                                  # reinvest_cg_short, reinvest_cg_long, cg_dist_short,
                                  # cg_dist_long, return_of_capital, split,
                                  # transfer_shares, shares_added, shares_removed,
                                  # cash_in, cash_out, fee, tax_withholding,
                                  # misc_income, misc_expense
date = "2025-01-10"               # trade date
settle_date = "2025-01-13"        # optional
security = "VTI"                  # ticker or name
quantity = "10"
price = "200"
commission = "5.00"               # buy and sell only
amount = "2005.00"                # positive; the action gives the direction.
                                  # Buy: total cost. Sell: net proceeds. Optional
                                  # for buy/sell/reinvest (shares × price ± commission).
split = "2:1"                     # split only: new:old
to_account = "IRA"                # transfer_shares only
lot_method = "fifo"               # sell, transfer, remove: override the default
lots = [                          # specific identification
    { from = "buy1", quantity = "5" },            # lot made by that ref
    { from = "move", index = 2, quantity = "1" }, # its 2nd lot in this account
]
acquired = "2019-03-15"           # shares_added: original acquisition date
transfer = "Checking"             # cash_in/cash_out: the other account
category = "Opening Balance"      # cash_in/cash_out, misc: a category instead
memo = "..."
```

`invest_edit` takes the same fields with `ref` (and `confirm` for a
reconciled cash posting); `invest_delete` takes `ref`. `price` records a
price (`security`, `date`, `price`). `import_prices` takes `csv` text
(optional `expect_count`); `seed_lots` takes `date` and `csv` text (MIG-120).
Multi-line CSV goes in a `"""` string.

A holding's sales, transfers, removals, splits, and returns of capital
stay in date order: anything dated before the latest of them is refused,
and a transaction changes or goes only while nothing follows it.

## Expectations

```toml
[expect]
txn_count = 2                     # transactions in the database

[expect.balances]                 # as of as_of
"Checking" = "3255.58"

[expect.ending_balances]          # including future-dated entries
"Checking" = "3155.58"

[expect.cleared_balances]
"Checking" = "-80.00"

[expect.available_credit]         # "none" if the account has no limit
"Visa" = "4966.75"

[expect.account_status]           # open or closed
"Old Savings" = "closed"

[expect.category_totals]          # the category's own postings
"Food:Groceries" = "344.42"

[[expect.register]]
account = "Checking"
rows = [
    { date = "2026-01-01", amount = "1000.00", balance = "1000.00" },
    { date = "2026-01-05", amount = "-184.32", balance = "815.68", ref = "costco", payee = "Costco", counterpart = "Food:Groceries" },
]
```

```toml
[[expect.schedules]]
ref = "rent"
next_due = "2026-08-01"           # nominal date, or "none" once ended
status = "active"                 # active or ended
left = 3                          # "# left" (after-count schedules)

[[expect.occurrences]]            # pending occurrences, due dates in order
ref = "rent"
from = "2026-01-01"
to = "2026-06-30"
dates = ["2026-01-01", "2026-02-01"]
```

```toml
[[expect.reconcile]]              # the account's session in progress
account = "Checking"
difference = "0.00"               # optional checks:
cleared_balance = "1400.00"
opening = "1380.00"               # Σ reconciled postings now
opening_expected = "1400.00"      # last statement's ending balance
changed = ["groceries"]           # refs of reconciled txns changed since
payments = 1                      # items listed (checked or not)
deposits = 2

[[expect.reconcile_history]]      # newest first; every row listed
account = "Checking"
rows = [
    { statement_date = "2026-01-31", statement_balance = "1400.00", opening_balance = "0.00", status = "finished", items = 3, total = "1400.00" },
]

[expect]
integrity = ["reconciled_balance_mismatch"]   # checks expected to fail
```

```toml
[expect.cash_balances]            # investment cash as of as_of; "none" when linked
"Brokerage" = "9985.00"

[expect.account_values]           # what the account list shows (market value)
"Brokerage" = "11585.00"

[[expect.holdings]]               # every position, by security name
account = "Brokerage"
total_value = "11585.00"          # optional: priced holdings + cash
rows = [
    { security = "VTI", shares = "5", basis = "1252.50", market_value = "1600.00", price = "320", stale = false },
]                                 # market_value and price may be "none"

[[expect.lots]]                   # open lots, oldest acquisition first
account = "Brokerage"
security = "VTI"
rows = [
    { acquired = "2025-06-01", quantity = "5", basis = "1252.50", per_share = "250.5", term = "long" },
]

[[expect.gains]]                  # every realized gain row, by date
account = "Brokerage"             # optional; all accounts when left out
rows = [
    { date = "2026-03-01", security = "VTI", acquired = "2025-01-10", quantity = "10", proceeds = "2996.67", basis = "2005.00", gain = "991.67", term = "long", taxable = true },
]                                 # return of capital beyond basis: acquired,
                                  # quantity, term = "none"

[[expect.income]]
account = "Brokerage"
total = "175.84"                  # optional
rows = [                          # security "" = income with no security
    { security = "AAPL", dividends = "2.60", cg_short = "1.00", cg_long = "10.00", total = "13.60" },
]                                 # also interest, other

[[expect.inv_register]]
account = "Brokerage"
rows = [
    { date = "2025-01-10", action = "Buy", amount = "-2005.00", cash_balance = "7995.00", security = "VTI", quantity = "10", ref = "buy1" },
]                                 # action as the register shows it ("Transfer In")

[[expect.allocation]]
accounts = ["Brokerage"]          # optional; every open investment account
total = "10580.00"
rows = [
    { class = "cash", value = "8730.00", percent = "82.51" },
]                                 # by value, largest first

[[expect.performance]]            # account totals; each field optional
account = "Brokerage"
realized = "200.00"
income = "30.00"
unrealized = "350.00"
total_gain = "580.00"
total_return = "23.20"            # percent
```

`expect.integrity` lists integrity checks (snake_case) that must fail, and
only those. Leave it out and the check must be clean.

Register rows must list every row, in register order (date, then entry
order). `date`, `amount`, `balance` are required; `ref`, `payee`,
`check_num`, `status` (`normal`/`void`), `cleared`, and `counterpart` are
checked when given. `counterpart` is a category path, `[Account]` for a
transfer, `--Split--`, or `""`.

## Harness actions

`add` / `subtract` with `amount`, and `expect.total` / `expect.today`,
test the runner itself (`harness/`).
