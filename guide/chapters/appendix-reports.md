# Reports in Detail {.appendix}

Each report, what it shows, and the choices it has beyond the date
range. Every report has **Totals only** in Customize, except Asset
Allocation. See [Reports](reports.md) for the window they share.

## Investing

**Capital Gains.** Every sale in the date range, lot by lot: shares,
date bought, date sold, proceeds, cost basis, and gain or loss,
short-term and long-term apart. For checking against the broker's
1099-B. **Subtotal by**: Short vs. long-term, Month, Quarter, Year,
Account, Security, or Don't subtotal. Filters: Accounts, Securities.
With no accounts chosen, it covers the taxable accounts only.

**Investment Performance.** How each account's investments did over
the date range: the internal rate of return and the time-weighted
return, which leaves out the effect of money moved in and out.
Filter: Accounts.

**Investment Income.** Dividends, interest, short- and long-term
capital gain distributions, and other income, by security and
account, whether paid in cash or reinvested. Filters: Accounts,
Securities.

**Holdings.** Each holding on a date: shares, cost basis, market
value, and unrealized gain. An account's own cash is a row of its
own. A holding with no price has no market value. Filters: Accounts,
Securities.

**Asset Allocation.** Market value by asset class on a date, as a
table and a chart. Investment cash counts as Cash. Click a class to
open Holdings for its securities. Filter: Accounts.

## Net Worth

**Net Worth.** Assets, liabilities, and net worth by account group,
on a date or over time, with a graph. **Interval** (in Customize):
None for one date, or a column per week, month, quarter, year, and
so on. **Accounts with zero balances** shows the empty ones. Filter:
Accounts; with a few accounts and Interval None, it's a list of their
balances.

## Spending

**Itemized Categories.** Every transaction in the range, grouped
under INCOME, EXPENSES, and TRANSFERS by category and subcategory,
with totals on each heading. **Sort by**: Date/Account,
Account/Date, or Amount; the Date, Account, and Num headings sort
too. **Subtotal by** (Customize): Category, or Tag, then category.
Transfers are off until you turn them on in Customize. Filters:
Accounts, Categories, Payees, Tags.

**Itemized Payees.** The same, grouped by payee.

**Income/Expense by Category.** Totals for each category, with
subcategories folded into their parents, for the range. **Interval**
spreads them into a column per week, month, quarter, or year, to see
trends. **Cash flow (transfers to other accounts)** adds a TRANSFERS
section listing money moved to or from accounts not chosen, such as
a loan payment or a deposit to a brokerage, so the total is what the
chosen accounts gained or lost. Filters: Accounts, Categories,
Payees, Tags.

**Income/Expense by Payee.** The same, totaled by payee.

## Comparison

**Current Spending vs. Average by Category.** Spending in the range
beside the average of the periods before it. **Date range** offers
weeks, months, quarters, and years: Current week, Last week, Current
month, and so on. **Compare to** offers periods to match:

| Date range | Compare to |
|:---|:---|
| A week | Last 4, 8, or 12 weeks |
| A month | Last 3, 6, or 12 months |
| A quarter | Last 4, 8, or 12 quarters |
| A year | Last year, Last 3 or 5 years |

The compared periods end on the range's last day, so they include it.
Periods with nothing count, at zero. Filters: Accounts, Categories,
Payees.

**Current Spending vs. Average by Payee.** The same, by payee.

## Tax

**Capital Gains.** The Investing report above, starting at Last year.

**Tax Schedule.** Amounts by tax form and line, such as Schedule A,
B, and D, with the transactions under each, from the taxable
accounts. Schedule D comes from the lots sold, by holding period.
The form rows are shaded and the totals sit on the heading lines.
**Date range** also offers **Monthly**, **Quarterly**, and
**Yearly**, for estimated taxes. Non-cash gifts to charity over $500
are marked **(Form 8283 needed)**. Filters: Accounts, Categories,
Payees, Securities.

**Tax Summary.** Every tax-related transaction: tax-related
categories, investment income, taxable gains, and withholding.
**Subtotal by**: Category (the default, with INCOME, EXPENSES, and
TRANSFERS), Tax line, Account, Payee, Tag, Month, Quarter, Year, or
Don't subtotal. **Sort by** as the itemized reports. Starts at Last
year, sorted Account/Date. Filters: Accounts, Categories, Payees,
Tags.

::: note
Kansha gathers the numbers for a return; it doesn't work out the
tax.
:::

## Date Ranges

| Choice | Covers |
|:---|:---|
| Include all dates | Everything in the book |
| Month to date, Quarter to date, Year to date | From the start of the period to today |
| Last month, Last quarter, Last year | The whole period before this one |
| Last 30 days, Last 12 months | Back from today |
| Custom dates… | Any from and to dates |
| Monthly, Quarterly, Yearly | Tax Schedule: a second list picks which one |

The comparison reports have their own list: Current and Last week,
month, quarter, and year. The first day of a week follows **Settings
> Interface**.
