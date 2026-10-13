# Categories, Payees, and Tags

::: {.note title="At a glance"}
A **category** says what money was for: groceries, salary, rent. A
**payee** says who it went to or came from. A **tag** marks
transactions across categories, such as everything for one trip.
Each has its own list under **Tools**, where you can rename, hide,
merge, and delete.
:::

## Categories

Every transaction line has a category, or a transfer account instead
(see [The Register](register.md)). The reports add up the categories
to show where money came from and went.

Each category is **Income** or **Expense**. Categories nest: a
category can sit under another, and its full name is the path, with
`:` between the levels, such as `Auto:Fuel` or `Tax:Fed`. A report
can show the parent with its subcategories folded in or spread out.

### The category list

Choose **Tools > Categories**.

::: {.shot id="category-manager"}
Tools > Categories with the list of categories on the left and the
selected category's Name, Kind, Parent, Tax-related, Tax line, and
Hidden fields on the right, with Merge into below.
:::

Click a category to see and change it:

| Field | What it's for |
|:---|:---|
| Name | The category's own name, without its parent. It can't contain `:` |
| Kind | Income or Expense |
| Parent | The category it sits under, or **(top level)** |
| Tax-related | Ticked when the category matters for taxes |
| Tax line | The line of the tax form it belongs on |
| Hidden | Keeps it out of the lists you pick from |

**New** adds a category. To make a subcategory, set its **Parent**.
To move a category, with everything under it, to another place,
change its Parent.

You can also make a category as you type it in the register: type a
name that isn't in the list, and Kansha asks whether to create it.

### Tax lines

A tax-related category with a tax line set shows up on the **Tax
Schedule** report under that line, such as Schedule A, medical and
dental, so the report reads like the form (see [Reports](reports.md)).
The Standard categories come with the usual ones set already.

### Built-in categories

A few categories belong to Kansha itself:

| Category | What it holds |
|:---|:---|
| Opening Balance | Each account's starting balance |
| Uncategorized | Lines entered with no category |
| Balance Adjustment | What reconciling adds to make an account match its statement |
| Tax Withheld | Tax held back from an investment payment |
| Dividends, interest, capital gains | Investment income, filled in by the investment register |

Only a built-in category's tax settings and **Hidden** can change.

### Merge, hide, or delete

- **Merge into** moves every transaction from this category to the
  one you choose, then removes this one. Use it to clean up two
  categories that mean the same thing.
- **Hidden** keeps a category you no longer use out of the lists,
  while its old transactions stay where they are. Reports still show
  it.
- **Delete** works only for a category no transaction uses. For
  anything else, hide it or merge it.

::: tip
Merging can't be taken back with Undo, but Kansha backs up the book
first. Look at the category in a report before you merge, so you
know what's moving.
:::

## Payees

A payee is created the first time you type a new name in the register.
Kansha remembers it, with the category, memo, and amount from its
last use. The next time you type that payee, it fills them in for
you. This is called a **memorized payee**.

Choose **Tools > Memorized Payees** to see the list. Each row shows
the payee and what its last use would fill in: **Category**, **Memo**,
and **Amount**. **Show all payees** includes the hidden ones.

Click a payee to:

- change its **Name**, which renames it on every transaction;
- tick **Hidden**, so it's no longer offered as you type;
- **Merge** it into another payee, so a misspelling such as
  `Kroger's` joins `Kroger`;
- **Delete** it, if no transaction uses it.

Settings decides how payees are remembered: whether to recall them
at all, whether new ones are memorized, whether names are
capitalized as you type, and whether payees unused for some months
are dropped (see [Settings and Themes](settings.md)).

## Tags

A tag marks transactions that belong together but have different
categories: everything for a vacation, a rental property, or a
wedding. Quicken calls them classes.

Choose **Tools > Tags** to list them. **New** adds one; click one to
change its **Name**, hide it, **Merge** it into another, or
**Delete** it.

In the register, type the tag in the **Tag** column. In a split,
each line can have its own tag. Every spending and tax report can be
filtered to one tag or a few (see [Reports](reports.md)).

::: tip
Use a tag instead of a new category when the question is "how much
did this one thing cost?" Groceries on vacation are still groceries;
tag them `Maine 2026` and the trip's total is one report away.
:::
