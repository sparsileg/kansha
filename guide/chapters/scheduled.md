# Scheduled Transactions

::: {.note title="At a glance"}
A **schedule** is a transaction that comes again and again: a
paycheck, the rent, the electric bill. Kansha reminds you when one is
due, or enters it by itself. **Reminders** lists every schedule; the
**Calendar** shows them on the days they fall, with the balance they
leave.
:::

## Reminders

Click **Reminders** on the navigation bar, or choose **Tools >
Reminders**.

::: {.shot id="reminders"}
The Reminders view: the New schedule and Due and overdue buttons, and
the table of schedules with Date due, Payee, Amount, Account, How
often, Remind, # left, End date, and Mode.
:::

Each row is one schedule, in order of the date it's next due:

| Column | What it shows |
|:---|:---|
| Date due | When it's next due |
| Payee, Amount, Account | What it enters, and where |
| Method | Payment or deposit, in the account's own words |
| How often | Monthly, every 2 weeks, and so on |
| Remind | How many days ahead it shows as due |
| # left, End date | When it stops, if it does |
| Mode | **Remind** or **Auto**; **estimate** when the amount is an average |

Click a row to change the schedule itself.

## Make a Schedule

Click **New schedule**. Or, in a register, right-click a transaction
you want to repeat and choose **Schedule this…**; the dialog opens
filled in from it.

::: {.shot id="schedule-dialog"}
The New scheduled transaction dialog for an electric bill: Account,
Payee, Transaction type, Amount set to Average of past 3 payments, the
category, and the Scheduling section set to Monthly on a day.
:::

### What it enters

- **Account**: the account it's entered in.
- **Payee** and **Memo**.
- **Transaction type**: payment or deposit (charge or payment for a
  card, and so on).
- **Amount and category**. The split button beside the category
  makes it a split, such as a paycheck with its deductions. A
  transfer account can stand in for the category, for a monthly move
  to savings or a card payment.

### The amount

**Amount** has two choices:

- **Fixed**: the same amount each time.
- **Average of past payments**: the average of the last few
  payments entered from this schedule, 3 unless you change it (1 to
  99). Kansha keeps it up to date as payments are entered, changed,
  or deleted. Use it for a bill that changes, such as electricity.
  You put in the real amount each time you enter one.

An average needs a single category, not a split, and the **Remind me**
mode, since someone has to check the real bill.

### When it's due

**Frequency**:

| Frequency | Example |
|:---|:---|
| Only once | A tax payment due April 15 |
| Daily | |
| Weekly | Every Friday, or every 2 weeks with **Every** set to 2 |
| Twice a month | Paid on the 15th and the last day |
| Monthly on a day | Rent on the 1st |
| Monthly on the last day | |
| Monthly on the Nth weekday | The second Tuesday, or the last Friday |
| Quarterly | Estimated taxes |
| Twice a year | Car insurance |
| Yearly | A subscription |

**First due on or after** is the first date it falls on. Kansha takes
the first date on or after it that fits the frequency. When you
change a schedule, the box reads **Next due on or after**.

**End**: **Never**, **On a date**, or **After a number of
occurrences**. With a number, **# left** counts down as each one is
entered.

### How it reminds you

**Remind days before due** is how early it shows as due: 3 shows a
bill three days before its date.

**Mode**:

- **Remind me**: it waits in **Due and overdue** for you to enter it.
- **Enter automatically (flag for review)**: Kansha enters it on its
  date by itself. **Due and overdue** then lists it under **Entered
  automatically: please review** until you click **Reviewed** (or
  **Mark all reviewed**). To change or void one, use the account's
  register.

Click **Save**.

::: note
Changes to a schedule apply to this occurrence and all the ones after
it. Transactions already entered from it stay as they are.
:::

## Due and Overdue

When something is due, Kansha opens **Due and overdue** as the book
opens. You can open it any time with the **Due and overdue (n)**
button in Reminders.

::: {.shot id="due-dialog"}
The Due and overdue dialog with three rows: an overdue rent payment,
an electric bill marked Estimate, and a paycheck, each with Enter,
Skip, and Edit… buttons.
:::

For each one:

- **Enter** opens the account's register with the transaction filled
  in. Check it, change the amount if it's an estimate, and save.
- **Skip** passes over this occurrence; the next one moves up.
- **Edit…** opens the occurrence's **Date** and **Amount**. Change
  them and click **Set for this occurrence only** to change this one
  occurrence, such as a paycheck that comes a day early for a
  holiday, without changing the schedule. The row then shows **This
  one edited**; **Undo the change** puts it back. (A split's amount is
  changed in the register after entering.)

Only the earliest occurrence of each schedule can be entered or
skipped; the later ones wait their turn.

If you delete a transaction that was entered from a schedule, the
occurrence comes back as due.

## The Calendar

Click **Calendar** on the navigation bar, or choose **Tools >
Calendar**.

::: {.shot id="calendar"}
The Calendar for one month with checking chosen and Projected balance
ticked: each day lists its scheduled transactions, with the balance
at the end of the day under it.
:::

The month shows each scheduled transaction on the day it falls. Use
the arrows to move a month, and **Today** to come back.

- **Account**: **All accounts**, or one checking, savings, cash, or
  credit card account.
- **Projected balance**, with an account chosen, shows the balance
  at the end of each day, counting what's entered and what's
  scheduled. Look for a day where it falls below zero, so you can
  move money in time.
- **Show entered transactions** adds what's already in the register.

Click a day to see what falls on it, and to **Enter**, **Edit**, or
**Skip** a scheduled one, or make a **New schedule** on that date.

::: tip
Before a big purchase, open the Calendar on checking with Projected
balance ticked. The lowest day of the coming weeks is how much you
can spend without going short.
:::
