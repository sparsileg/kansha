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
                                  # other_asset, other_liability, loan (investment types: Phase 6)
credit_limit = "5000.00"          # optional, credit cards only
opening_balance = "-500.00"       # optional, ledger sign (negative = owed)
opening_date = "2026-01-01"       # required with opening_balance

[[categories]]
path = "Food:Groceries"           # parents are created too
kind = "expense"                  # expense or income
```

Payees and tags are created the first time an action names them.

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

Register rows must list every row, in register order (date, then entry
order). `date`, `amount`, `balance` are required; `ref`, `payee`,
`check_num`, `status` (`normal`/`void`), `cleared`, and `counterpart` are
checked when given. `counterpart` is a category path, `[Account]` for a
transfer, `--Split--`, or `""`.

## Harness actions

`add` / `subtract` with `amount`, and `expect.total` / `expect.today`,
test the runner itself (`harness/`).
