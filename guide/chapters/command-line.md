# Entering from the Command Line

::: {.note title="At a glance"}
`kansha add` enters a purchase from a terminal without opening the
app. Entries also come as files, from a script or sent from a phone.
None of them touches your accounts until you approve it in Kansha,
under **File > Review Entries…**.
:::

## What Can Be Entered

Spending only: purchases, bills, and refunds, split or not, in an
open checking, savings, cash, or credit card account, with an
**expense** category. Transfers, income, and investment transactions
are entered in the app.

::: why
An entry from outside is **unverified**. It waits in a list of its
own and changes no balance, register, or report until you approve
it. That way a mistyped command or a file from the wrong phone can't
change your books behind your back.
:::

## kansha add

::: note
The command line is for Kubuntu, where the installer puts `kansha`
on the path. On Windows, use **File > Import Entries…** with a file
instead.
:::

Type `kansha add` in a terminal, and it asks for each field:

```
$ kansha add
Date [today]: 
Payee: Shell
Amount: 45.20
Account: Double Cash
Category (or split): Auto:Fuel
Memo (Enter for none): Ohio trip
accepted (unverified): 1 entry for household
```

- **Date**: press Enter for today, or type `yesterday`, a date like
  `2026-10-12`, or one in the book's date format.
- **Amount** can be a sum, as in the register: `3.99*2+1.50`. Type
  `=` to work it out before moving on. A refund is negative.
- **Account** and **Category**: press **Tab** to complete the name.
  The end of a name is enough when only one name ends that way:
  `Groceries` finds `Food:Groceries`. A wrong name is asked again,
  with the close matches.
- **Category (or split)**: type `split` to split the amount. Kansha
  then asks for each line, `Line 1 amount (182.40 left):`, and its
  category, until nothing is left. A line can be typed as the amount
  and category together: `120.00 Groceries`.

### In one line

For a script, or once you know the format, give the whole entry on
the command line:

```
kansha add today Shell 45.20 Auto:Fuel acct "Double Cash"
```

The entry format is in [The Entry File Format](appendix-kansha-add.md).
A name with spaces goes in quotes.

### Options

| Option | What it does |
|:---|:---|
| `--book <path>` | The book's `.db` file. Without it, `KANSHA_DB`, else the last book opened |
| `--from <name>` | Who sent it, shown at review. Without it, `command line` |
| `kansha run <file>` | Reads a file of entries |
| `kansha run -` | Reads entries from standard input |
| `kansha help` | Shows the usage |

### Where entries go

`kansha add` doesn't open the book and needs no passphrase. It
writes the entries to the book's inbox, a folder beside the book
named `<book>-inbox`, and answers `accepted (unverified)`. That's all
it ever answers: no balances, nothing from the book.

It checks names against the **names copy**, `<book>-names.txt`
beside the book. Kansha writes it each time the book closes. If
there's no copy yet, the command says names weren't checked, and the
review in Kansha checks them instead.

## Entry Files

A file of entries starts with the line `kansha-entries 1`:

```
kansha-entries 1
from "Stan's phone"
2026-10-12 Shell 45.20 Auto:Fuel acct "Double Cash"
2026-10-12 Costco 182.40 split acct Checking
  120.00 Food:Groceries
  62.40 Household memo "paper goods"
```

Bring one in with **File > Import Entries…**, or drop it in the
book's inbox folder. Kansha watches the inbox while the book is open,
and reads it when the book opens.

A file goes in whole or not at all: if any line is wrong, Kansha
lists each bad line by its number and takes none of them.

### From a phone

**File > Export Names for Phones…** writes a `kansha-names 1` file:
your expense categories and open spending accounts, no payees and no
amounts. A phone app can use it to offer the right names, then send
back an entries file by email or a chat app. Save the file and use
**File > Import Entries…**.

::: note
Entry files and the names file are plain text, not encrypted, until
Kansha reads them. They hold only what you entered and the names of
categories and accounts.
:::

## Review Entries

When the book opens with entries waiting, Kansha shows **Review
entries**. Otherwise, **File > Review Entries… (n)** shows how many
are waiting.

::: {.shot id="review-entries"}
The Review entries dialog with three entries: From, Date, Payee,
Amount, Account, Category, and Memo, each with Approve, Edit, and
Reject, and one row marked ⚠ for a category name that doesn't match.
:::

For each entry:

- **Approve** makes it an ordinary transaction in its register.
- **Approve and open** approves it and opens it in the register, to
  change a split.
- **Edit** changes it in the list first. A name marked **⚠** doesn't
  match the book; fix it here before approving.
- **Reject** throws it away.
- **Later** leaves the rest for another time.

**Undo** takes back an approval: the transaction goes and the entry
is back in the list.

Each entry has an ID, so a file sent twice adds nothing the second
time.
