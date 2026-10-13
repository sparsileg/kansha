# Investments

::: {.note title="At a glance"}
An investment account holds securities as well as cash. Each buy,
sell, dividend, or reinvestment is entered in the account's register
through a dialog that knows what the action needs. Kansha keeps the
**lots** (which shares were bought when, and for how much) so it can
work out capital gains. The **Investments** screen shows every
holding at today's prices.
:::

## Investment Accounts

The investment types are Brokerage, Traditional IRA, Roth IRA, HSA,
401(k)/403(b), and Donor Advised Fund (see [Accounts](accounts.md)).
Each has:

- its **holdings**: shares of securities, in lots;
- its **cash**, kept in the account itself or in a linked checking
  or savings account;
- a **tax treatment**, which decides whether its income and gains
  show on the tax reports;
- a **default lot method**, which decides which lots a sale takes.

## Securities

A security is a stock, fund, bond, or anything else with shares and
a price. Choose **Tools > Securities** to list them.

::: {.shot id="securities"}
Tools > Securities with the list on the left and one fund's Name,
Ticker, Type, Class, and the Prices table on the right, with Import
prices…, Seed lots…, and True up lots… below.
:::

| Field | What it's for |
|:---|:---|
| Name | The security's name |
| Ticker | Its symbol, such as `VTI`; price downloads use it |
| Type | Stock, ETF, mutual fund, bond, money market, CD, or other |
| Class | Its asset class, for the Asset Allocation report |
| CUSIP | Optional |
| Notes | Anything you like |
| Hidden | Keeps an old security out of the lists and price downloads |

**New security** adds one; a new security takes its type's usual
asset class.

**Delete** works only for a security no transaction uses; hide it
otherwise.

## The Investment Register

Click the account in the Accounts panel.

::: {.shot id="investment-register"}
A brokerage register: rows with Date, Action, Security, Shares,
Price, Comm., Amount, Cash, Clr, and Memo, a reinvestment and a sale
among them, and the line New transaction: type or click here at the
bottom.
:::

| Column | What it holds |
|:---|:---|
| Date | The trade date |
| Action | Buy, Sell, Dividend, and so on |
| Security | What was bought, sold, or paid |
| Shares, Price, Comm. | The shares, the price per share, and the commission |
| Amount | The money involved |
| Cash | The account's cash after this transaction |
| Clr | `c` or `R`, as in any register |
| Memo | A note |

A cash balance below zero shows **⚠ below zero**: usually a missing
deposit or a sale entered after the buy it paid for.

## Enter a Transaction

Click the line **New transaction: type or click here**, or start
typing on it. The entry dialog opens. Choose the **Action** first;
the fields change to suit it.

::: {.shot id="investment-entry"}
The investment entry dialog for a Sell: Trade date, Security, Shares,
Price per share, Commission, Net proceeds, and the lot picker listing
the open lots with Shares to take.
:::

| Action | What it records |
|:---|:---|
| Buy | Shares bought with the account's cash; enter **Total cost** |
| Sell | Shares sold; enter **Net proceeds** |
| Dividend, Interest | Cash paid by a security |
| Reinvest dividend | A dividend used to buy more shares |
| Reinvest ST / LT capital gain | A capital gain distribution used to buy more shares |
| ST / LT capital gain distribution | A fund's capital gain paid as cash |
| Return of capital | Cash paid back, which lowers the cost basis |
| Stock split | **Old shares** become **New shares**; cost stays the same |
| Transfer shares | Shares moved to another of your investment accounts, lots and all |
| Shares added | Shares that arrived without cash: enter **Cost basis** and **Originally acquired** |
| Shares removed | Shares that left without cash, such as a gift to charity |
| Roth conversion | Shares or cash moved from a traditional IRA to a Roth |
| Cash in, Cash out | Money moved into or out of the account |
| Fee | A fee charged to the account |
| Tax withheld | Tax held back from a payment |
| Misc income, Misc expense | Anything else |

The fields every action shares:

- **Trade date** and **Settlement date** (optional).
- **Shares**, **Price per share**, and **Commission**. Kansha works
  out shares × price, with the commission, and shows it under the
  amount; leave the amount empty to take it.
- **Memo**.

The buttons at the bottom:

- **Enter/Next** saves and opens a fresh dialog for the next one.
- **Enter/Done** saves and closes.
- **Reset** clears the dialog; **Cancel** closes it without saving.
- When changing an existing transaction: **Delete** and
  **History…**.

::: note
A dividend, interest, or capital gain distribution needs no
category: Kansha files it under its built-in investment income
categories, where the Investment Income and Tax Schedule reports
find it.
:::

### Gifts of shares

To give shares to a charity, choose **Shares removed**, and under
**Given to (optional; needs the price)** name the charity and enter
the price on the day. Kansha records the gift at shares × price,
with no gain, and the Tax Schedule flags non-cash gifts over $500 as
**(Form 8283 needed)**.

### Roth conversions

Choose **Roth conversion** in the traditional IRA. Enter the **Value
converted**, the **To Roth IRA** account, any **Federal tax withheld**
and **State tax withheld**, and the **Nontaxable part** if some of
the money was already taxed (from Form 8606 or the plan's 1099-R).
With no security chosen, the conversion is in cash. Tax withheld is
paid from the IRA's cash on top of the value converted; leave it
empty if you pay the tax from elsewhere.

## Lots and Lot Methods

A **lot** is one purchase of a security: its date, its shares, and
what they cost. When you sell, the shares come out of one or more
lots, and the gain is the sale price minus what those lots cost.
Shares held more than a year are long-term; the rest short-term.

**Lot method** decides which lots a sale takes:

| Method | Takes |
|:---|:---|
| First in, first out | The oldest lots first |
| Choose lots | The lots you pick in the dialog |
| Average cost | Shares at the average cost of all lots (mutual funds) |
| Highest cost first | The most expensive lots, for the smallest gain |
| Minimum tax | Losses first (short-term, then long-term), then long-term gains, then short-term gains |

Each account has a **Default lot method** (see
[Accounts](accounts.md)), and each security in it can have its own.
**Settings > Investments** sets the method for new accounts.

For **Choose lots**, the sell dialog lists each open lot with its
**Acquired** date, **Open shares**, and **Term**. Type the number to
sell from each under **Shares to take**.

::: why
The IRS lets you choose which shares you sold, as long as you tell
your broker at the time. If your broker sold specific lots, choose
the same ones here, so Kansha's gains match the 1099-B.
:::

### Matching your broker

Kansha's lots can drift from your broker's: an old import with no
lots, a sale where the broker chose different shares, or a cost basis
the broker adjusted. To fix a holding:

1. Download the cost-basis list (unrealized gains, by lot) from the
   broker as CSV.
2. In **Tools > Securities**, pick the security and click **True up
   lots…**.
3. Choose the **True-up date**, and the file, or paste its text into
   **Or paste the CSV**.
4. Click **Compare**. Kansha lists its lots beside the broker's.
5. Click **True up**. Kansha adjusts its lots to match, with the
   **Memo** you give.

For a traditional IRA, Roth, HSA, or 401(k), where the cost doesn't
matter for taxes, only the shares are compared.

**Seed lots…** starts holdings from a CSV with one row per lot:
account, security, acquired, quantity, cost basis. Each lot becomes
a **Shares added** transaction on the seeding date and keeps its
acquired date. Use it for shares from before your records start.
**Preview** checks the file first.

## Prices

Kansha values each holding at the latest price it has.

- **Download prices**, on the Investments screen, fetches today's
  prices for every security that has a ticker and isn't hidden. It
  needs **Allow price download (internet)** ticked in **Settings >
  Investments**; until then Kansha never goes online.
- **Tools > Import Prices…** reads a file of prices: one line per
  security, `ticker, price`, with an optional date as `MM/DD/YYYY`.
  Drop the file on the dialog or click **Choose file…**.
- In **Tools > Securities**, the **Prices** table takes a price by
  hand: date, price, **Add**.

A price older than 7 days counts as stale, and **Needs attention** on
Insights says so. **Settings > Investments** changes the number of
days.

## The Investments Screen

Click **Investments** on the navigation bar.

::: {.shot id="investments-view"}
The Investments screen: the bar with View, As of, Download prices,
and the gear, and the table of holdings with
Quote/Price, Shares, Cost Basis, Market Value, Gain/Loss, and Day
Gain/Loss.
:::

The table lists each holding with its **Quote/Price**,
**Shares**, **Cost Basis**, **Market Value**, **Gain/Loss** in money
and percent, and **Day Gain/Loss**.

- **As of** shows the holdings on another date.
- **View** switches among five views: **Default** and **Custom 2**
  through **Custom 5**.
- The gear's **Customize…** sets what a view shows: its **Columns**,
  **Accounts**, and **Securities**. **Reset view** puts it back.
- **Show closed lots**, in the gear, includes lots already sold.

Click a security to open its details: a graph of its market value or
price history over a range you choose, with **Fit graph to data** to
zoom in on the line.

::: tip
Set up **Custom 2** with only your retirement accounts and **Custom
3** with only the taxable ones. Switching views is then one click.
:::
