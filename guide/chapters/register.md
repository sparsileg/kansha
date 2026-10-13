# The Register

::: {.note title="At a glance"}
Each account has a register: its transactions, one per row, oldest
at the top, with the running balance down the right. Enter a new one
on the line at the bottom. Click a row, or press Enter on it, to
change it. Most of the work in Kansha happens here.
:::

This chapter covers the register of a bank, cash, card, loan, or
other account. Investment accounts have their own register; see
[Investments](investments.md).

## Open a Register

Click the account in the Accounts panel. The register opens with the
newest transactions in view and the entry line ready at the bottom.

::: {.shot id="register"}
A checking account register: the filter bar on top, rows with Date,
Num, Payee, Payment, Deposit, Category, Tag, Memo, Clr, and Balance,
a split row, future rows in italics below the line at today, the
entry line at the bottom, and the footer with Cleared, Current, and
Ending balances.
:::

## The Columns

| Column | What it holds |
|:---|:---|
| Date | When the transaction happened |
| Num | A check number, or anything you like such as `ATM` or `EFT` |
| Payee | Who the money went to or came from |
| Payment, Deposit | The amount, in the column for its direction |
| Category | The category, a transfer account, or `--Split--` |
| Tag | An optional tag (see [Categories, Payees, and Tags](categories.md)) |
| Memo | A note |
| Clr | `c` once cleared, `R` once reconciled |
| Balance | The account's balance after this transaction |

The two amount columns are named for the kind of account:

| Account | Money out | Money in |
|:---|:---|:---|
| Checking, savings, money market | Payment | Deposit |
| Credit card | Charge | Payment |
| Cash | Spend | Receive |
| Loan, other liability | Increase | Decrease |
| Other asset | Decrease | Increase |

Click a column heading to sort by it; click it again to reverse the
order. The heading shows ▲ or ▼. The line at today shows only when
the register is sorted by date.

Transactions dated after today are in italics, below a line that
marks today. They're in the **Ending** balance, but not yet in
**Current**.

### The footer

- **Cleared**: the balance of the cleared and reconciled
  transactions. It should match the bank's balance, give or take
  what hasn't reached the bank yet.
- **Current**: the balance as of today.
- **Ending**: the balance after the last transaction, future ones
  included.
- **Available credit**, for a credit card with a credit limit.

## Enter a Transaction

Click the entry line at the bottom, or press **Ctrl+N** or
**Insert**. Fill in the fields left to right, pressing **Tab** to
move on, then press **Enter** or click **Enter**.

**Date.** Type it in the book's date format. A short date is
completed: `3/5` is March 5 of this year. In the date field, **+**
and **-** move the date a day forward or back, and **t** sets it to
today.

**Num.** Optional.

**Payee.** As you type, Kansha offers the payees that start with what
you've typed. When you pick one it already knows, it fills in the
category, memo, and amount from the last time; change any of them as
needed.

**Amount.** Type it in one of the two columns. The amount can be a
sum: `12.50+3.25` or `3*4.99`. Type `=` or move on, and Kansha works
it out.

**Category.** Kansha offers matches as you type, one level at a time:
type `au`, take `Auto`, then `:`, and only Auto's subcategories are
offered. To move money to another account, pick the account instead;
accounts are listed after the categories (see Transfers below).

**Tag** and **Memo.** Optional.

::: tip
Press **Escape**, or click **Cancel**, to throw away what you've
typed on the entry line.
:::

### Warnings

Kansha may ask before saving a transaction that looks like a mistake:

- its date is more than 7 days ago or more than 30 days ahead;
- its check number is already used in this account;
- a line has no category.

Each warning can be turned off in **Settings > Notifications** (see
[Settings and Themes](settings.md)).

## Splits

A split is one transaction with more than one category: a store
receipt that was part groceries and part household goods, or a
paycheck with salary, taxes, and a 401(k) contribution.

Click the **Split** button at the end of the entry line, or choose
`--Split--` as the category. The split panel opens.

::: {.shot id="split-panel"}
The split panel under a paycheck: lines for Salary, Tax:Fed,
Tax:State, and a transfer to the 401(k), each with Amount, Tag, and
Memo, the Add line button, and Remainder showing a check mark.
:::

Give each part a category or transfer account and an amount. A part
going the other way, such as the taxes taken out of a paycheck, is
typed with a leading `-`. **Add line** adds another line.

**Remainder** shows what's left to assign. When the lines add up to
the transaction's amount it shows **✓**; until then, **✗** and the
amount left. A transaction is saved only when the remainder is zero.

::: tip
For a paycheck, enter the net pay that reached the bank as the
amount, then split it: gross salary as a positive line, and each
deduction as a negative line. The tax reports then see every part.
:::

## Transfers

Money moved between two of your accounts, such as a credit card
payment from checking, is a **transfer**. Enter it once, in either
account, and put the other account in the **Category** column.
Kansha writes it in both registers.

In the other register, it shows with the account you entered it from
as its category. To jump there, right-click the row and choose **Go to
other side of transfer**.

::: why
A transfer isn't income or spending. It only moves money between
your own accounts, so it changes no category and leaves net worth as
it was. Entering it as a transfer, not as an expense in one account
and income in the other, keeps the reports honest.
:::

## Change a Transaction

Double-click a row, or select it and press **Enter**. It opens for
editing where it is; change it as you would a new one, then click
**Save** or press **Enter**. Enter then moves on to the next row, so
you can work down the register.

Right-click a row, or press **Shift+F10** or the Menu key, for more:

| Command | What it does |
|:---|:---|
| Edit | Opens it for changing |
| Split | Opens it with the split panel |
| Mark cleared / Mark unmarked | Sets or clears the `c` in Clr |
| Go to other side of transfer | Opens the other account's register at this transaction |
| Schedule this… | Makes a schedule from it (see [Scheduled Transactions](scheduled.md)) |
| History… | Every change made to it: the field, before, and after |
| Void | Keeps the transaction but makes its amount zero |
| Delete | Removes it |

**Undo** (**Ctrl+Z**, or **Edit > Undo**) takes back the last change
in a register: an entry, a change, or a delete.

### The keyboard

With a row selected:

| Key | What it does |
|:---|:---|
| ↑ ↓ | Previous or next row |
| Home, End | First or last row |
| Page Up, Page Down | A page up or down |
| Enter | Change the row |
| Space | Mark it cleared, or unmark it |
| Delete | Delete it |
| Insert, Ctrl+N | Go to the entry line |
| Shift+F10, Menu key | The right-click menu |

## Filter the Register

The bar above the rows narrows what's shown: **From** and **To**
dates, **Payee**, **Category**, **Tag**, and **Cleared** (All,
Unmarked, Cleared, or Reconciled). **Clear all** shows everything
again.

## Find and Replace

**Ctrl+F**, or **Edit > Find**, puts the cursor in the search box on
the navigation bar. It finds transactions in every account by payee,
category, memo, note, check number, account name, or amount.

**Edit > Find/Replace…** finds transactions that match one or more
conditions and changes them all at once:

1. Choose a field (Amount, Category, Check #, Cleared status, Date,
   Memo, Payee, or Tag), a comparison (Contains, Exactly, Starts
   with, Ends with, Greater, Less, and so on), and the value.
2. **Add row** for more conditions, joined by **And** or **Or**.
   **Show matches within splits** lists matching split lines.
3. Tick the matches to change, or use **Select all**.
4. Choose the **Replace** field, type what goes in **With:**, and
   click **Replace all**.

::: tip
Kansha backs up the book before **Replace all**, and one Undo
reverses the whole replacement.
:::

## Cleared and Reconciled

When a transaction shows on your bank statement or online banking,
mark it cleared: press **Space** on it, or right-click and choose
**Mark cleared**. Clr shows `c`.

Reconciling with a statement changes cleared transactions to `R`
(see [Reconciling](reconciling.md)). Reconciled transactions show in
gray, which Settings can turn off. Changing one asks first, since it
would undo work you've checked against a statement.

A closed account's transactions can't be changed until the account
is reopened (see [Accounts](accounts.md)).
