# Insights

::: {.note title="At a glance"}
**Insights** is the home page: tabs of cards, each card a small
report that keeps itself up to date. A new book starts with one tab,
**Status**. Add tabs, choose their cards, and make spending cards of
your own.
:::

## Open Insights

Click **Insights** on the navigation bar, or choose **Tools >
Insights**. It's also what Kansha opens to at startup, unless
Settings says otherwise (see [Settings and Themes](settings.md)).

::: {.shot id="insights"}
The Status insight: the Net worth card with three year-end columns,
This month, Net worth over time, Due soon, and Needs attention with
its check mark.
:::

## The Cards

| Card | What it shows |
|:---|:---|
| Net worth | Each account group, and net worth, today and at the last two year-ends |
| This month | This month's income and spending so far |
| Net worth over time | A graph over 1, 2, or 5 years; **Fit graph to data** zooms in on the line |
| Due soon | Scheduled transactions due in the next 14 days |
| Needs attention | Problems in the book, each with where to fix it |
| Spending cards | Spending in the categories you choose; see below |

**Due soon** looks ahead 14 days unless Settings says otherwise.

### Needs attention

Kansha runs a set of checks each time Insights opens:

- the last integrity check, the last backup, and whether it was
  verified;
- whether the backup folder is there;
- prices that are stale or missing;
- overdue scheduled transactions;
- transactions not cleared after 60 days;
- accounts not reconciled in 60 days;
- uncategorized transactions;
- investment cash below zero.

When all pass, the card shows **✓ No problems found.** and how many
checks passed. Otherwise it shows **⚠** and lists each problem.
**Show checks** lists every check, passed or not.

::: tip
Glance at Needs attention once a week. It catches the small things,
such as a payment with no category, before they add up.
:::

## Spending Cards

A spending card totals the categories you choose: everything for the
house, or food in all its subcategories. It shows the **Year**, the
**Month**, and the **Monthly Avg**, one row per category.

To make one, open the insight's gear, choose **Customize…**, and click
**New spending card…**. Give it a **Name**, then tick its
**Categories**, and on the **Accounts** tab the accounts it counts.

A spending card has its own gear with **Customize…** and **Delete
card…**. **Settings > Interface** sets how many rows a spending card
shows before it scrolls (10 to start with).

## Arrange Insights

The gear at the end of the tabs holds:

- **Customize…**: the insight's **Name**, and its cards. **Available
  cards** lists the cards not on it; **On this insight (in order)**
  lists those on it. **Add ›** and **‹ Remove** move a card between
  the two; **▲ Up** and **▼ Down** change the order.
- **Create new insight…**: a new tab.
- **Move left**, **Move right**: the tab's place.
- **Delete insight…**: removes the tab.

::: tip
Make a second insight called **Taxes**, with spending cards for
charity, medical, and the other deductions you track. In December it
answers "how are we doing for the year?" at a glance.
:::
