# Coming from Quicken

::: {.note title="At a glance"}
Export your whole Quicken file as one QIF file, then import it with
**File > Import…**. Kansha shows what it found before it changes
anything, lets you test the import first, and can take a whole import
back out again. Schedules are entered again by hand.
:::

Kansha reads the QIF files Quicken 2013 exports. One file brings your
accounts, categories, tags, payees, securities, prices, and every
transaction, with both sides of each transfer matched up. Investment
history comes too, and Kansha rebuilds every lot from it.

## Before You Start

Make a fresh book for the import, with **Start with** set to
**Empty** (see [Getting Started](getting-started.md)). The import
brings its own categories, so a standard list would only get in the
way.

::: tip
Import into a throwaway book first, called something like `trial`.
Check it over. If you don't like what you see, delete the book's two
files and start again. When the result is right, make your real book
and import into that.
:::

## Export from Quicken

In Quicken 2013:

1. Choose **File > Export > QIF File**.
2. Under **Quicken account to export from**, choose **All
   Accounts**.
3. Leave the dates at their widest, so everything comes.
4. Tick every box under **Include in export**: transactions, the
   account list, the category list, memorized payees, and the
   security lists.
5. Save the file somewhere you can find it, such as
   `quicken-all.qif`.

::: why
A QIF of one account holds only that account's transactions: no
account names, categories, tags, or securities, and only one side of
each transfer. The whole-file export holds all of it, so Kansha can
match every transfer to its other side and import it once.
:::

Kansha can read a single-account QIF too, for a quick trial; its
transactions go to an account named after the file.

## Import the File

Choose **File > Import…** and click **Choose QIF file…**. Kansha reads
the file and shows the **Import from Quicken** window. Nothing has
been changed yet; the file waits in memory until you click
**Import**.

::: {.shot id="import-preview"}
The Import from Quicken window after reading a file: the summary of
records, dates, and counts at the top, the Accounts table with its
Import as choices, and the Categories, tags, securities section.
:::

### The summary

The top of the window says what the file holds: how many
transactions, how many transfers were matched, the new payees, the
prices, the dates the transactions run from and to, and the memorized
transactions that are skipped.

**Date order.** QIF files don't say whether `3/4/26` is March 4 or
April 3. Kansha looks for a date with a day over 12 to decide. When
no date decides it, it assumes month first and says so with a ⚠; if
that's wrong, choose **Day first (31/1/26)** under **Date order**.

### Accounts

One row for each account in the file, with its records, its dates,
and what it adds to the balance. For an investment account that's
the cash only.

For each account, choose what **Import as** does:

- **New account**: Kansha creates it, with the name and type shown.
  You can change both.
- **Existing account**: the transactions go into an account already
  in the book.
- **Skip**: the account is left out. Use this for Quicken's hidden
  accounts that you don't want to bring along. A transfer to a
  skipped account goes to Opening Balance instead.

Kansha matches each QIF account to the book's account of the same
name when there is one.

### Categories, tags, and securities

Each category, tag, and security the transactions use is listed, with
what it maps to: a new one, or one already in the book. You can map a
category to an existing one to merge two on the way in, or give it a
new path to rename it.

Only what the imported transactions use is created. To keep the
others too, such as categories you used before the dates in the
file, tick **Show the … the imported transactions do not use** and
tick the ones to keep.

A security that no account holds after the import, an old holding,
is created hidden, so price downloads pass it by. Tick it to keep it
shown.

**Import … prices of the securities kept** brings Quicken's price
history along. Leave it ticked unless you don't want old prices.

### Problems and notes

**Cannot be imported** lists records Kansha can't read, each with its
line number in the file. They stop the import, unless you tick
**Import the rest and leave these out**.

**Notes** lists what Kansha noticed but can import anyway: sections
it doesn't import (budgets and the like), transfers to accounts left
out, and transfers with only one side in the file.

Some choices stop the import until they're fixed: two QIF accounts
onto one Kansha account, a banking account onto an investment one, a
name or ticker already taken, a category under one of the other kind.
Kansha says which.

## Test, Then Import

Click **Test import**. Kansha runs the whole import, shows the
result, and then puts everything back as it was. Nothing is kept.

The result lists each account's balance before and after, against
what the file says it adds, and marks any difference.

When the test looks right, click **Import**. Kansha backs up the book
first, then imports everything in one step: either all of it goes
in, or none of it does.

::: note
Kansha keeps a copy of every file it imports, encrypted like a
backup, in a folder beside the book called `<book>-imports`. If you
import a file that was imported before, the preview says so.
:::

## Check the Result

Compare Kansha with Quicken before you rely on it:

1. **Balances.** Each account's balance in the Accounts panel should
   match Quicken's account list on the day you exported.
2. **Categories by year.** Run **Reports > Spending >
   Income/Expense by Category** with **Interval** set to **Year**,
   and compare a few years against the same report in Quicken.
3. **Investments.** Run **Reports > Investing > Holdings** and compare
   shares and cost basis with Quicken's portfolio view.

Lines that had no category in Quicken are in **Uncategorized**. The
Needs attention card on Insights lists them by account, so you can
fix them when you have time.

## Lots and Your Broker

QIF files carry no lots. Kansha rebuilds them from the buys, sells,
and reinvestments, using each account's lot method. For most holdings
the result matches your broker's. Where it doesn't, the broker's
records are the ones the IRS sees.

To make a holding match, download its cost-basis list from the
broker as CSV, then use **True up lots…** in **Tools > Securities**
(see [Investments](investments.md)).

## Undo an Import

**File > Import… > Past imports…** lists every import with its file
and the number of transactions. **Roll back…** deletes that import's
transactions, with the accounts, categories, payees, tags, and
securities it created that nothing else uses. The book is backed up
first.

## What Doesn't Come Across

- **Scheduled transactions.** Enter your bills and paychecks again in
  Reminders (see [Scheduled Transactions](scheduled.md)). It's a good
  time to tidy them.
- **Hidden accounts**, unless you import them. QIF doesn't mark which
  accounts are hidden, so skip them yourself in the Accounts table.
- **Budgets**, reports, and Quicken's settings.
- **Older Quicken files.** If you started a new Quicken file at some
  point, only the current one is imported.

## Quicken Words in Kansha

| Quicken | Kansha |
|:---|:---|
| Data file | Book |
| Class | Tag |
| QuickFill, memorized payee | Memorized payee |
| Scheduled bill or deposit | Schedule (Reminders) |
| Bills and Income Reminders | Due and overdue |
| Home page | Insights |
| Investing Center | Investments |
| Account Bar | Accounts panel |
| Toolbar | Navigation bar |
| Tax line item | Tax line |
