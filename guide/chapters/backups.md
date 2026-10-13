# Backups and Restore

::: {.note title="At a glance"}
Kansha backs up the book by itself: when it closes, a few minutes
after a change, and before anything big. Each backup is one
encrypted file in the backup folder, opened with the book's
passphrase. **File > Restore…** brings a book back from one, and
shows what you'd gain or lose first.
:::

## How Backups Work

A backup is a single `.zip` file holding the whole book, encrypted
with the backup passphrase. Its name says which book, when, and why:

```
household-20261012-183005Z-close.zip
```

The time is in UTC (the `Z`). The last word is the kind:

| Kind | Made |
|:---|:---|
| manual | When you choose **File > Back Up Now** |
| close | When the book closes |
| timeout | A few minutes after a change, while you work |
| bulk | Before a change to many transactions, such as a merge or Replace all |
| import | Before an import |
| migration | Before a new version of Kansha updates the book |
| restore | Before a restore replaces the book |

::: why
A backup made just before a big change means the change can always
be taken back, however it goes. That's why Kansha doesn't ask: the
backup costs a second and could save the book.
:::

## Settings > Backups

Choose **Edit > Settings…** and **Backups**.

::: {.shot id="settings-backups"}
Settings, Backups page: the Backup folder with Browse… and Use
Downloads, the three numbers for keeping backups, and the Backup
tools list with Apply.
:::

- **Backup folder**: where every backup goes. **Browse…** picks
  another; **Use Downloads** goes back to Downloads. If the folder
  goes missing, such as a USB drive that isn't plugged in, Kansha
  says **⚠ This folder is missing…**.
- **Keep the newest automatic backups**: 10 to start with.
- **… plus one per month for (months)**: 12 to start with, so a year
  of month-end backups stays too.
- **Back up after a change (minutes, 0 = off)**: 5 to start with.

Kansha removes older automatic backups beyond those. **Manual
backups are never removed**; delete them yourself when you no longer
want them.

::: tip
Make the backup folder one that's copied off the computer: a cloud
folder such as Dropbox or OneDrive, or a network drive. The backups
are encrypted, so a cloud service can store them but can't read
them.
:::

### Backup tools

Pick one from **Backup tools** and click **Apply**:

- **Change backup passphrase…**: a new passphrase for the book and
  for backups made from now on. Older backups keep the passphrase
  they were made with; keep a note of it as long as you keep them.
- **Show database key…**: after the passphrase, shows the key that
  unlocks the book file itself. With it, a database tool such as DB
  Browser for SQLite (SQLCipher build) can open the book to look,
  not to change.
- **Verify backup…**: opens a backup, checks it, and says **✓ The
  backup opens and passes the integrity check.**

::: tip
Once a month, run **Verify backup…** on the newest backup. A backup
you've never opened is only a hope.
:::

## Restore

Choose **File > Restore…**, or **Restore from a backup…** on the
passphrase screen, and click **Choose backup file…**.

::: {.shot id="restore"}
The Restore from backup window: the backup's Book, Made, Kind, Kansha
version, and Schema version, and the table comparing transactions
and balances in the backup with the current book.
:::

Kansha opens the backup and shows:

- the **Book** it came from, when it was **Made**, its **Kind**, and
  the **Kansha version** and **Schema version** that made it;
- each account's **Transactions** and **Balance**, in the backup and
  in the current book;
- what's **Only in the backup**, and what's **Only in the current
  book (lost by restoring)**.

**Show only rows that differ** hides the accounts that match.

Restoring replaces the whole book with the backup. Kansha backs up
the current book first, as a **restore** backup, so even a restore
can be undone.

To restore another book's backup into this one, tick **Replace this
book with the other book's data**. Kansha asks so it doesn't happen
by mistake.

::: note
A backup from a newer version of Kansha can't be opened by an older
one. Install the newer Kansha first.
:::

## Move to Another Computer

1. On the old computer, choose **File > Back Up Now**.
2. Copy that backup to the new computer, or use the shared backup
   folder.
3. Install Kansha on the new computer. On the **Set up** screen,
   choose **Restore from a backup**, pick the file, and type its
   passphrase.

Or copy the book's two files, `.db` and `.key`, and open them with
**Open another book…** (see [Getting Started](getting-started.md)).

## Integrity Check

**File > Integrity Check** checks the whole book: every transaction
balances, each holding's shares equal its open lots, lots keep their
cost basis, and nothing points at something that isn't there. It
only looks; it changes nothing. It ends with **No problems found.**
or a list of what's wrong. **Settings > Data** can run it each time
the book opens.

## History

Every change to the book is recorded. **History…**, in a register's
right-click menu, the investment entry dialog, or the account
dialog, lists a transaction's or account's changes: the **Field**,
the value **Before**, and the value **After**.

## New Books and Year-End

**File > New…** makes another book. Besides **Empty** and **Standard
categories**, it can start as a **Copy of this book**, with a choice
of balances:

- **Zero**: the accounts and categories, no transactions.
- **Carry each balance**: each account starts with its balance on
  the **carry date**.
- **Carry, and keep transactions after the carry date**.

**File > Rename Book…** renames the book and its files; it asks for
the passphrase.

::: tip
There's no need to start a new book each year. A Kansha book holds
decades without slowing down, and reports pick any dates you like.
:::
