# Something Doesn't Look Right

::: {.note title="At a glance"}
The common surprises, and what to do about them. Start with
**Needs attention** on Insights; it finds many of these by itself.
:::

## Balances

**The balance doesn't match the bank.** Compare the register's
**Cleared** balance with the bank's, not **Current**: the bank hasn't
seen what's still on its way. Then reconcile against the last
statement (see [Reconciling](reconciling.md)).

**The balance is different from yesterday's, and I didn't change
anything.** Look for a transaction dated today that was entered
ahead, or a schedule set to **Enter automatically**. **Current**
moves to include future-dated rows as their day comes.

**A reconciled account's opening balance changed.** A reconciled
transaction was changed or deleted. The Reconcile view lists it under
**⚠ Opening balance changed.**; put it back as it was.

**Net worth looks too low (or too high).** An investment's price may
be old or missing: see **Needs attention**. Or a card or loan's
starting balance went in the wrong column, so it counts as money you
have instead of money you owe; check its Opening Balance
transaction.

## Entering

**Kansha asks "Create new expense category…?" when I type a
category.** The name isn't in the list. Say no and pick the right one,
or yes to make it.

**The payee didn't fill in the category.** QuickFill fills only an
empty field, only on a new transaction, and only for a payee it has
memorized. Check **Recall memorized payees** in **Settings >
Register**, and that the payee isn't hidden.

**The transaction won't save: a split.** The split lines must add up
to the amount. **Remainder** shows what's left; add a line or fix an
amount until it shows ✓.

**Kansha asks before I save.** One of the notifications caught it: a
date far from today, a check number used before, or a missing
category. Check, then save, or turn the warning off in **Settings >
Notifications**.

**I can't change a transaction.** If its account is closed, reopen
the account first. If it's reconciled, Kansha asks before changing
it. An investment transaction is changed in the investment register,
not from the other side of a transfer.

**I deleted the wrong thing.** **Edit > Undo** (**Ctrl+Z**) takes back
the last register change. For anything bigger, restore a backup from
before it (see [Backups and Restore](backups.md)).

## Investments

**Shares or cost basis don't match the broker.** Lots drift for many
reasons. Use **True up lots…** with the broker's cost-basis CSV (see
[Investments](investments.md)).

**Cash shows ⚠ below zero.** A buy is entered before the money that
paid for it, or a deposit is missing. Look at the rows just above
the first negative cash.

**Download prices does nothing.** Tick **Allow price download
(internet)** in **Settings > Investments**. A security needs a ticker
and must not be hidden.

**A sale's gain is different from the 1099-B.** The broker sold
different lots. Edit the sale, choose **Choose lots**, and pick the
broker's lots.

## Reports

**A report total looks wrong.** Click it to see the transactions
behind it. Look for one in the wrong category, the wrong date, or a
transfer counted as spending.

**A category is missing from a report.** Check **Customize… >
Categories**: it may be cleared. Hidden categories show only with
**Show hidden and closed** ticked.

**Capital gains for an IRA don't show.** They aren't taxed, so the
Capital Gains report leaves out tax-deferred and tax-exempt accounts
unless you choose them in **Customize… > Accounts**.

## Schedules

**A bill didn't show as due.** Check its **Remind days before due**
and its next date in **Reminders**. A schedule whose **End** has
passed, or with no occurrences left, is finished.

**I can't enter next month's bill.** Only the earliest occurrence of
a schedule can be entered or skipped. Enter or skip this month's
first.

## Backups and Opening

**The passphrase isn't accepted.** Passphrases are exact: check Caps
Lock and the spaces. If you changed the passphrase, an older backup
still needs the old one.

**⚠ This folder is missing.** The backup folder isn't there: a USB
drive unplugged, or a network drive not connected. Connect it, or
choose another in **Settings > Backups**.

**Kansha says the book was made by a newer version.** Install the
newest Kansha. A book that a newer version has updated can't go back
to an older one.

**The window opens too small, or off the screen.** Kansha remembers
the window's size and place for this computer. Close Kansha and
delete `config.json` in the settings folder (see [Getting
Started](getting-started.md)) to start fresh. The theme, font, and
list of recent books start fresh too; the books aren't touched.

## Still Stuck

**File > Integrity Check** checks the whole book and lists anything
wrong. If it finds something you can't fix, restore the last backup
from before the problem started.
