# Your First Month

::: {.note title="At a glance"}
This chapter follows one month of household money through Kansha:
setting up accounts, entering spending, letting reminders handle the
bills, reconciling the statements, and seeing where it all went.
Each step points to the chapter with the details.
:::

The example is a household with a checking account, a credit card,
and a paycheck twice a month. Yours will differ in the details, but
the rhythm is the same.

## Day 1: Set Up

1. **Make the book.** Start Kansha, name the book, choose a backup
   folder off this computer, and set a passphrase you've written
   down. Choose **Standard categories**, unless you're importing from
   Quicken (see [Getting Started](getting-started.md) or [Coming
   from Quicken](coming-from-quicken.md)).
2. **Add the checking account.** Click **New account…**, name it,
   leave **Type** at **Checking**, and save. Its register opens.
3. **Enter its starting balance.** Take the balance from your last
   statement. In the register's empty line, enter the statement's
   date, the payee `Opening Balance`, the balance under **Deposit**,
   and the category **Opening Balance**.
4. **Add the credit card** the same way, with **Type** set to
   **Credit card**. Its starting balance is what you owe, under
   **Charge**.

See [Accounts](accounts.md) for the other kinds of account.

## Week 1: Enter What You Spend

Each time you spend or receive money, enter it in the register of
the account it came from (see [The Register](register.md)):

- Click the empty line at the bottom, or press **Ctrl+N**.
- Type the date, the payee, the amount, and the category, then press
  **Enter**.

The second time you type a payee, Kansha fills in the category,
memo, and amount from the last time. Most entries then take a few
keystrokes.

::: tip
A grocery run that was partly household goods is a split: one
transaction, two categories. Click the **Split** button at the end
of the entry line and give each part its own line. See [The
Register](register.md).
:::

Away from the computer? Enter it from a terminal or your phone, and
approve it the next time Kansha is open (see [Entering from the
Command Line](command-line.md)).

## Week 1: Set Up Reminders

Bills and paychecks that come again and again don't need typing each
time. Open **Reminders**, click **New schedule**, and fill in:

- the paycheck: a deposit to checking, **Twice a month** on the days
  you're paid;
- the rent or mortgage: a payment, **Monthly on a day**;
- the electric bill: a payment whose amount changes. Set **Amount**
  to **Average of past payments**, and Kansha estimates it from the
  last few bills.

From then on, when a bill is due Kansha shows it under **Due and
overdue**. Click **Enter**, check the amount against the real bill,
and save. See [Scheduled Transactions](scheduled.md).

The **Calendar** shows the month with every bill and paycheck on its
day. Tick **Projected balance** with checking chosen to see whether
the account stays above zero all month.

## End of the Month: Reconcile

When the bank statement comes, open **Reconcile**, choose checking,
and enter the statement's date and ending balance. Tick each
transaction that's on the statement. When **Difference** reaches
0.00, click **Finish**. Do the same for the credit card statement.

A difference that won't go away is almost always a transaction
missing on one side, entered twice, or typed with the digits
swapped. See [Reconciling](reconciling.md).

## End of the Month: Look Back

- **Insights** shows the month on one page: net worth, this month's
  income and spending, what's due soon, and anything that needs
  attention (see [Insights](insights.md)).
- **Reports > Spending > Income/Expense by Category** with the date
  range at **Last month** lists where the money went. Click any
  amount to see the transactions behind it.
- **Reports > Comparison > Current Spending vs. Average by
  Category** shows which categories ran over their usual.

See [Reports](reports.md).

## Every Month After

The rhythm settles into a few minutes a week:

1. Enter what you spend, or approve what you entered on your phone.
2. Enter the reminders as they come due.
3. Reconcile each statement when it arrives.
4. Glance at Insights. Anything under **Needs attention** says what's
   wrong and links to where to fix it.

Kansha backs up the book by itself all along. Once in a while, check
that the backups are where you expect them (see [Backups and
Restore](backups.md)).
