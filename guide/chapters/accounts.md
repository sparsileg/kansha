# Accounts

::: {.note title="At a glance"}
An account is anywhere money is kept or owed: a checking account, a
credit card, a brokerage account, a house. Add one with **New
account** in **Tools > Accounts**. Its starting balance is the first
transaction in its register. The type is chosen once and can't be
changed later.
:::

## Account Types

| Type | Use it for | Kind |
|:---|:---|:---|
| Checking | A bank account you write checks or pay bills from | Asset |
| Savings | A savings account | Asset |
| Money market | A bank money market account | Asset |
| Cash | Cash in your wallet or a jar | Asset |
| Credit card | A credit card or store card | Liability |
| Brokerage | A taxable investment account | Asset |
| Traditional IRA | An IRA or rollover IRA | Asset |
| Roth IRA | A Roth IRA | Asset |
| HSA | A health savings account | Asset |
| 401(k)/403(b) | An employer retirement plan | Asset |
| Donor Advised Fund (DAF) | A charitable fund you advise | Asset |
| Other asset | A house, a car, money someone owes you | Asset |
| Other liability | Money you owe that isn't a loan or card | Liability |
| Loan | A mortgage, car loan, or student loan | Liability |

An **asset** adds to your net worth; a **liability** takes away from
it. The six investment types (Brokerage through DAF) have an
investment register that tracks securities, shares, and lots as well
as cash (see [Investments](investments.md)). The rest have the
ordinary register (see [The Register](register.md)).

::: why
The type decides which register an account has, how its columns are
named, and how it's treated in reports and for taxes. Changing it
later would change the meaning of every transaction already in it,
so Kansha fixes it when the account is made. If you chose wrong, make
a new account of the right type and move the transactions.
:::

## Add an Account

Choose **Tools > Accounts** and click **New account**. (In a new book,
the **New account…** button on the empty screen does the same.)

::: {.shot id="account-dialog"}
The New account dialog for a checking account: Name, Type, Group, Tax
treatment, Opening date, Institution, Account number with its Reveal
button, Interest rate, and the Show in account list box.
:::

Only **Name** and **Type** are needed. The rest can be filled in now
or later:

| Field | What it's for |
|:---|:---|
| Name | What the account is called everywhere in Kansha |
| Type | See the table above. Fixed once saved |
| Group | Which section of the Accounts panel it's listed in |
| Tax treatment | Taxable, Tax-deferred, or Tax-exempt; see below |
| Tax line, transfers out / in | The tax form line money moving out of or into it belongs on |
| Opening date | The day the account was opened |
| Institution, Phone, Web address | Where the account is held |
| Account number | Shown as dots; **Reveal** shows it |
| Description, Notes | Anything you want to remember |
| Interest rate (%) | Checking, savings, and money market accounts |
| Credit limit | Credit cards; the register shows the credit left |
| Sort order | The account's place within its group |
| Show in account list | Untick to keep an account out of the Accounts panel |

Click **Save**. The new account appears in the Accounts panel.

### The starting balance

There is no opening balance field. The starting balance is an
ordinary transaction, the first one in the register:

- **Date:** the date of the statement you take the balance from.
- **Payee:** `Opening Balance`.
- **Amount:** the balance, under **Deposit** for a bank account, or
  under **Charge** for what you owe on a card.
- **Category:** **Opening Balance**.

::: why
As a transaction, the starting balance has a date, shows in the
register, and can be reconciled against the first statement like
anything else. Opening Balance is a built-in category that reports
leave out of income and spending, so the starting money doesn't look
like a paycheck.
:::

An investment account starts with its holdings instead; see
[Investments](investments.md).

### Tax treatment

**Tax treatment** says how the IRS sees money in the account:

- **Taxable**: a bank or brokerage account. Its interest, dividends,
  and capital gains go on your tax return.
- **Tax-deferred**: a traditional IRA or 401(k). Nothing is taxed
  until it's taken out. New IRA and 401(k) accounts start here.
- **Tax-exempt**: a Roth IRA, an HSA, or a DAF. New Roth, HSA, and DAF
  accounts start here.

The tax reports and capital gains reports use it to leave out what
isn't taxed (see [Reports](reports.md)).

### Investment settings

Investment accounts have a few more fields:

- **Cash**: **Internal** keeps the account's cash inside it.
  **Linked** keeps it in a separate checking or savings account
  chosen under **Linked cash account**, as some brokers do.
- **Money market funds**: whether a money market fund in the account
  counts as cash or as a security.
- **Default lot method**: which shares a sale takes when you don't
  choose them yourself (see [Investments](investments.md)).

### Other assets

An **Other asset** has a **Kind**: house, vehicle, or other. A house
or car bought with a loan can name that loan as its **Linked
liability**.

## The Accounts Panel

The Accounts panel down the side of the window lists the open
accounts with their balances, in sections: Banking, Credit,
Investments, Retirement, Assets & debt, and Other. Net worth is at
the bottom. Click an account to open its register.

- **Show closed accounts** at the bottom brings back closed ones.
- The gear opens **Arrange accounts**. **▲** and **▼** move an
  account up or down its section; the list beside it moves the
  account to another section.
- Settings can show the balances with or without cents (see
  [Settings and Themes](settings.md)), and put the panel on the left
  or the right.

## Change, Close, or Delete

To change an account's details, open **Tools > Accounts** and click
**Edit…** beside it. From a register, the gear (**Account options**)
has **Edit account details…**.

At the bottom of the account dialog:

- **Close account** puts away an account you no longer use. Its
  transactions stay, and reports still count them. Its balance
  should be zero first, with no shares left; if not, Kansha asks
  before closing it. **Reopen** brings it back.
- **Delete** removes the account entirely. It works only on an
  account with no transactions.
- **History…** lists every change made to the account's details:
  the field, the value before, and the value after.

::: tip
Close an account rather than deleting it. A closed account keeps its
history for reports and taxes, and stays out of the way.
:::
