# Introduction

*Know where your money went. Trust every number.*

Kansha keeps your household accounts: checking, savings, credit
cards, loans, and investments. You enter what you spend and earn,
Kansha remembers the bills that come every month, and you reconcile
each account against its statement. For investments it keeps every
lot with its cost basis, so capital gains come out right at tax
time. Reports and the Insights cards show where the money went, what
you're worth, and what goes on your tax return.

Kansha was built to replace Quicken 2013. It reads the QIF files
Quicken exports, so years of history come with you (see [Coming from
Quicken](coming-from-quicken.md)).

*Kansha* is Japanese for *gratitude*; the brushstrokes on the
cover are the word in Japanese script.

::: {.note title="Quick Start"}
The first time Kansha starts, it asks for three things: a name for
your book, a folder for its backups, and a passphrase. Then add an
account and start entering transactions. [Getting
Started](getting-started.md) has the details.

If you'd rather look around first, choose **Load sample data** in a
new, empty book. It fills the book with three years of made-up
accounts and transactions to try things on.
:::

## Getting Help as You Go

Hold the pointer over a button or a choice and a tooltip says what
it does. Many choices in dialogs also say when to pick them.

This guide is for when you want more than a tooltip: what a screen is
for, how to read it, and what to do next. [Your First
Month](first-month.md) follows a month of household money from start
to finish. [Recipes](recipes.md) walks through common jobs, and
[Something Doesn't Look Right](troubleshooting.md) explains the most
common surprises. The [Glossary](appendix-glossary.md) defines the
terms.

**Help > User's Guide** opens this guide from inside Kansha.

## What Do You Want to Do?

Start from your question and go to the screen that answers it.

| I want to… | Go to |
|:---|:---|
| Bring my Quicken history into Kansha | [Coming from Quicken](coming-from-quicken.md) |
| Add a bank, card, loan, or investment account | [Accounts](accounts.md) |
| Set up the categories my spending goes into | [Categories, Payees, and Tags](categories.md) |
| Enter what I spent or earned | [The Register](register.md) |
| Have Kansha remember bills and paychecks | [Scheduled Transactions](scheduled.md) |
| See what's coming up and what my balance will be | [Scheduled Transactions](scheduled.md), the Calendar |
| Check an account against its statement | [Reconciling](reconciling.md) |
| Record buying, selling, and dividends | [Investments](investments.md) |
| See where my money went | [Reports](reports.md), [Insights](insights.md) |
| Get the numbers for my tax return | [Reports](reports.md), Tax reports |
| Enter a purchase from the terminal or my phone | [Entering from the Command Line](command-line.md) |
| Keep my data safe, or move it to another computer | [Backups and Restore](backups.md) |
| Change dates, colors, or what opens at startup | [Settings and Themes](settings.md) |

## Your Data Stays With You

Everything you enter is stored on your own computer, in one file
called a *book*. No account is needed, and nothing is sent anywhere.
Kansha never connects to the internet unless you turn on **Allow
price download** in Settings, and then it sends only the ticker
symbols of the securities whose prices it fetches.

The book is encrypted. Without your passphrase, the file can't be
read, by anyone.

::: why
Encryption protects you if the computer or a backup drive is lost or
stolen. It also means that **nobody can recover your book if you
forget the passphrase**: not Kansha, not its author, not a computer
shop. Keep the passphrase in a password manager, or written down
somewhere safe, from the day you set it.
:::

Kansha backs up the book by itself: when you close it, a few minutes
after you change something, and before any big change such as an
import. A backup on the same disk doesn't survive a failed disk, so
choose a backup folder somewhere else: a USB drive, a network drive,
or a cloud-synced folder (see [Backups and Restore](backups.md)).

## Moving Around

::: {.shot id="main-window"}
The main window with a checking account's register open: the menu
bar, the navigation bar with its buttons and the Search box, the
Accounts panel on the left, and the status bar.
:::

Across the top is the **menu bar**: File, Edit, Tools, Reports, and
Help. At its right end are the book's name and the theme, font, and
text size lists.

Under it is the **navigation bar**. Its buttons go straight to the
screens you use most; to start with, Insights, Reminders, Calendar,
Reconcile, and Investments. You choose its buttons and their order
in **Edit > Navigation Bar…**: any menu item or any account can have
a button. The Reminders button shows how many reminders are due.

At the left end of the navigation bar are **Back** and **Forward**.
They step through the screens you've visited since the book was
opened, like a web browser's. Hold the pointer over one to see where
it goes. At the right end is the **Search** box: type a payee, a
category, a memo, a check number, or an amount to find every
transaction that matches.

The **Accounts** bar sits under the navigation bar. Click the
triangle at its left to open the Accounts panel, the list of all
your accounts with their balances, grouped by type; click an account
to open its register. With the panel closed, click the rest of the
bar to drop the list down instead. The gear at its right arranges
the list. **Edit > Settings…** puts the panel on the left or the
right.

On the same row, on the other side, is the **status bar**. Kansha
reports there when it has made a backup or saved an export. A note
clears itself after 30 seconds. An alert, such as a backup that
failed, is in bold, flashes, and stays for a minute.

### Windows and the dock

Reports, the Calendar, Reminders, Reconcile, and Investments each
open in a window that fills the space under the bars. Several can be
open at once. The ones underneath wait in the **dock**, a row of
labels at the bottom of the main window; click a label to bring that
window back to the top. Opening an account's register leaves the
windows in the dock.

### Keyboard shortcuts

| Keys | What they do |
|:---|:---|
| Ctrl+N | A new transaction in the register |
| Ctrl+Z | Undo the last change in a register (outside a text field) |
| Ctrl+F | Put the cursor in the Search box |

On a Mac, use Cmd for Ctrl. The register itself works entirely from
the keyboard (see [The Register](register.md)).
