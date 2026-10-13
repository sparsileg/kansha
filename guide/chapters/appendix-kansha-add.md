# The Entry File Format {.appendix}

The `kansha-entries 1` format, used by `kansha add`, `kansha run`,
**File > Import Entries…**, and files sent from a phone (see
[Entering from the Command Line](command-line.md)).

## A File

```
kansha-entries 1
from "Stan's phone"
# Ohio trip
2026-10-12 Shell 45.20 Auto:Fuel acct "Double Cash" memo "Ohio trip"
2026-10-12 Costco 182.40 split acct Checking
  120.00 Food:Groceries
  62.40 Household memo "paper goods"
2026-10-14 Costco -24.99 Household acct Checking memo "return"
```

- The first line is `kansha-entries 1`: the format and its version.
- `from "<sender>"`, optional, names who sent the file. Review shows
  it.
- Then one entry per line. Blank lines are skipped.

## An Entry

```
<date> <payee> <amount> <category or split> acct <account>
    [memo "<text>"] [id <uuid>]
```

| Part | Rules |
|:---|:---|
| date | `2026-10-12`. `kansha add` and `kansha run` also take `today`, `yesterday`, and the book's date format |
| payee | Any text. A new payee is made when the entry is approved |
| amount | `45.20`. Negative for a refund: `-24.99` |
| category | An existing expense category, or `split` |
| `acct` | Required: an open checking, savings, cash, or credit card account |
| `memo` | Optional |
| `id` | Optional unique ID; Kansha makes one if it's missing. An entry whose ID was seen before is skipped |

`acct`, `memo`, and `id` can come in any order after the category.

## Splits

With `split` as the category, the split lines follow, indented:

```
<amount> <category> [memo "<text>"]
```

Every line has its amount, and the lines add up to the entry's
amount.

## Words and Names

- Words are separated by spaces. A name with spaces goes in double
  quotes: `"Double Cash"`. Inside quotes, `\"` is a quote and `\\` a
  backslash.
- A name spelled like one of the format's own words (`acct`, `memo`,
  `id`, `split`) is quoted too.
- `#` outside quotes starts a comment, to the end of the line.
- Case doesn't matter in names. The end of a category's name is
  enough when only one category ends that way: `Groceries` for
  `Food:Groceries`.
- A name that matches nothing, or more than one, is an error that
  lists the close matches. Kansha never guesses and never creates a
  category from an entry.

## Rules for a File

- A file goes in whole or not at all. Every bad line is listed by its
  number.
- The largest file read is 1 MB.

## kansha add Answers

| Answer | Means |
|:---|:---|
| `accepted (unverified): 2 entries for household` | Written to the inbox, waiting for review |
| `names not checked: there is no names copy yet …` | Close the book in Kansha once to make the copy; until then review checks the names |
| An error listing close matches | Nothing was saved; fix the name and try again |

## The Names File

**File > Export Names for Phones…** writes a `kansha-names 1` file:
the expense categories, with their subcategories, and the open
spending accounts. It holds no payees and no amounts. Kansha keeps a
copy beside the book, `<book>-names.txt`, written each time the book
closes, for `kansha add` to check names against.
