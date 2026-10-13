# Reconciling

::: {.note title="At a glance"}
Reconciling checks an account against its statement. Enter the
statement's date and ending balance, tick each transaction the
statement shows, and when the difference is zero, click **Finish**.
Then you know Kansha and the bank agree, up to that date.
:::

## Why Reconcile

A transaction typed with two digits swapped, entered twice, or never
entered at all throws off every balance after it. Reconciling finds
it while the statement is in front of you, a month at a time, instead
of a year later.

Once an account is reconciled, its transactions up to the statement
date are marked `R` and checked. The next reconciliation starts from
there.

## Start

Click **Reconcile** on the navigation bar, or choose **Tools >
Reconcile**, and pick the account.

::: {.shot id="reconcile-start"}
The Reconcile view's New statement section for checking: Statement
date, Ending balance, the optional Interest earned and Service charge,
and the Start reconciling button.
:::

Under **New statement**:

1. **Statement date** and **Ending balance**, from the statement.
   For a credit card, enter the balance you owe as the statement
   shows it.
2. **Interest earned** (optional; **Finance charges** for a card):
   the amount, date, and category, if the statement shows some you
   haven't entered.
3. **Service charge** (optional): the same, for a bank fee.
4. Click **Start reconciling**.

Kansha enters the interest and the charge as transactions, so you
don't have to.

## Tick What the Statement Shows

::: {.shot id="reconcile-session"}
Reconciling checking: the figures at the top with Difference, and
two tables below, Payments and checks on the left and Deposits on
the right, with some rows ticked.
:::

Two tables list the account's transactions that aren't reconciled
yet: money out on one side, money in on the other. Their names follow
the account: **Payments and checks** and **Deposits** for a bank,
**Charges and cash advances** and **Payments and credits** for a
card.

Go down the statement. For each item on it, click its row in Kansha;
the tick shows it's on the statement. The box at the top of a table
ticks or clears all of it.

The figures at the top keep count:

| Figure | What it is |
|:---|:---|
| Opening balance | The ending balance of the last reconciliation |
| Checked payments, Checked deposits | The ticked items on each side, with their count |
| Cleared balance | The opening balance plus everything ticked |
| Statement ending balance | What you typed from the statement |
| Difference | Statement ending balance minus cleared balance |

When **Difference** is 0.00, click **Finish**. The ticked
transactions become `R`, and the session is kept in the account's
history.

**Update** changes the statement date or ending balance, if you typed
one wrong.

You can leave in the middle. The session is kept, marked **(in
progress)** in the account list, and picks up where you left off the
next time.

## When It Won't Balance

Work through these, in order:

1. **Missing transaction.** Something is on the statement but not in
   Kansha. Go back to the register, enter it, and come back; it's in
   the list, ready to tick.
2. **Extra transaction.** Something is ticked that isn't on the
   statement. Compare the counts: the statement says how many
   checks and deposits it holds.
3. **Wrong amount.** If the difference divides evenly by 9, two digits
   are probably swapped: 45.10 for 54.10.
4. **Entered twice.** Look for a difference that equals one
   transaction's amount.
5. **Wrong statement.** The opening balance should match the
   statement's starting balance. If not, see below.

When you can't find it and the difference is small, **Balance
Adjustment…** enters a transaction for the difference, in the
built-in **Balance Adjustment** category, so you can finish. It
shows only while the difference isn't zero.

::: why
A balance adjustment makes the account agree with the bank without
saying why it didn't. Use it for the cents you've given up on, not
for a difference you could still find; the transaction it hides may
matter for taxes.
:::

**Abandon** throws the session away. Nothing is marked reconciled.
Interest or a service charge it entered stays in the register;
delete it there if you don't want it.

## Opening Balance Changed

If a reconciled transaction is changed or deleted after the fact, the
opening balance of the next reconciliation no longer matches the
last statement. Kansha shows **⚠ Opening balance changed.** and
lists the reconciled transactions that changed since then, so you
can put them right.

## History

Under the account, the history table lists each reconciliation: its
**Statement date** and whether it's **Finished**, **In progress**, or
**Abandoned**. **Show items** lists the transactions it reconciled.
