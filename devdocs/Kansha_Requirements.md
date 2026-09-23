# Kansha personal finance app

#kansha #quicken

<br>A custom cross-platform personal finance app to replace Quicken.

<br>

A useful first version could handle:

#### 1\. Accounts

- Checking
- Savings
- Credit cards
- Taxable brokerage
- Traditional IRA (tax-deferred)
- Roth IRA (tax-deferred)
- Cash/money-market accounts
- Other assets/liabilities

Each account has its own transaction history and current balance. Investment accounts can hold multiple equities.

### User Interface

* Dropdown at the top of the UI with a list of accounts to show in the ledger view. The account list can be toggled to drop down as a sidebar alongside the ledger.

* Account edit modal with account creation, edit, and deletion. Can toggle displaying account in account bar and list. Can close the account.

* Checking & savings edit has name, description, financial institution, account number, tax-deferred, interest rate, contact phone, home page.

* Credit card account has name, description, financial institution, account number, credit limit, phone, home page.

* Investment taxable account has name, description, financial institution, account number, phone, home page, tax-deferred, show cash in checking account, type.

* IRA/Roth/HSA has name, description, financial institution, phone, home page, tax-deferred, type, tax-deferred.

* Other assets (house, car, etc.) has name, description, phone, home page, type, tax-deferred, mortgage acct (for house), loan acct (for other loans)

#### 2\. Expenses and income

Transactions such as:

```
2026-09-22   Costco             -$184.32   Groceries
2026-09-22   Vanguard           +$1,247.18 Dividends
2026-09-23   Checking → Brokerage -$5,000  Transfer
```

With:

- categories
- subcategories
- payees
- notes
- tags
- splits
- transfers
- recurring transactions
- attachments, potentially later

### User Interface

A calendar display to view upcoming, usually recurring, transactions and, optionally, display completed transactions as well.

The transaction display has multiple rows of past transactions with the following columns: Date, Payee, Charge/Payment, Category, Tag, Memo, Clr, Balance

#### 3\. Investment transactions

This is where things get more interesting, but it's still very manageable.

You'd want:

- Buy
- Sell
- Dividend
- Interest
- Reinvest dividend
- Reinvest capital gain
- Return of capital
- Stock split
- Transfer shares
- Transfer cash
- Fees
- Tax withholding
- Download of current or past price

And then derive:

- shares held
- cost basis
- market value
- unrealized gain/loss
- realized gain/loss
- income by security
- asset allocation
- investment performance

#### 4\. Security master

Something like:

```
VTI
Vanguard Total Stock Market ETF
US Equity
Ticker: VTI
```

with historical prices stored separately.

You could also support mutual funds, ETFs, stocks, bonds and cash equivalents without making the underlying model much more complicated.



## 5. Recurring transactions

I use this heavily in Quicken so there needs to be a robust interface, coupled with the calendar view, to do CRUD operations on recurring transactions

The UI should include Date Due, Payee, Amount, How long before due to notify, #left (in the case of loans), Method (deposit/payment), How often (weekly, bi-weekly, monthly, twice a month, quarterly, twice a year, yearly, only once), end date. For recurring, list every # (week,month,etc). Recurring setup must be robust and cover all possibilities.

* * *

## The really important architectural decision

I'd make the **transaction ledger the source of truth**.

Don't store things like:

> "Current account balance = $137,482.23"

as the fundamental data.

Instead:

```
Account
   ↓
Transactions
   ↓
Calculated balance
```

Likewise for investments:

```
Investment Account
       ↓
Security Transactions
       ↓
Lots / Shares
       ↓
Positions
       ↓
Market Value / Gain
```

That gives you something very powerful: **you can reconstruct the financial state of the account at any historical date.**

That's one of the things I'd consider essential in a Quicken replacement.



## What it won't have

Some of these may be added, so keep things modular

* Import from external institutions (at least not yet)

* Financial and tax planning

## What it will have

* Import of QIF/QXF from Quicken. I believe QXF import is for transactions. Investment data will be imported via QIF one account at a time.

* Calculate net worth based on account balances

* Reports (details to follow)

* Account reconcilation

* Setting/Preferences

* Have an Icon bar to put oft-used links (reconcile, investments, calendar, etc.)



# Where it gets difficult

*  

### 2\. Investment cost basis

This is probably the most complicated part of the accounting model.

For example:

```
Buy 100 VTI @ $200
Buy 50 VTI  @ $220
Sell 75 VTI @ $250
```

The application needs to know which shares were sold and calculate the resulting gain.

Then add:

- reinvested dividends
- transfers between accounts
- stock splits
- partial sales
- inherited shares
- different tax lots
- wash sales
- return of capital

and it becomes a real accounting engine.

But you don't have to implement everything initially.

I'd build **tax lots + FIFO/specific-lot identification** first.

* * *

### 3\. Data correctness

This is probably more important than the UI.

A finance application needs to be able to answer:

> "Why does this account say $842,173.19?"

and let you drill all the way down to the transactions that produced that number.

I'd therefore put considerable effort into:

- immutable transaction IDs
- double-entry-ish accounting internally
- reconciliation
- audit history
- transaction import matching
- duplicate detection
- explicit corrections rather than silently changing historical data

That makes the system trustworthy.

 

## Reports

Reports must be robust to allow for tax estimation, tithing calculation, status of any or all accounts, net worth, and spending.

Be able to save reports (based on settings)

Have both tables and graphs

# One thing I'd do differently from Quicken

I'd make the application **account-centric rather than screen-centric**.

For example, an investment account could have tabs:

```
Overview | Transactions | Holdings | Lots | Income | Performance
```

And a household dashboard could show:

```
NET WORTH
$8,370,421

Assets
  Investments       $8,120,000
  Cash                 $183,000
  Other                 $67,000

This Month
  Income               $12,420
  Expenses              $7,814
  Net                   $4,606
```

Then investment reporting could be much better than traditional Quicken-style reports.

 

### My assessment

I'd roughly divide the project this way:

| Component                     | Difficulty |
| ----------------------------- | ---------- |
| Accounts & transactions       | 🟢         |
| Categories & expense tracking | 🟢         |
| QIF/CSV import                | 🟢         |
| Basic investment tracking     | 🟢         |
| Security/price database       | 🟡         |
| Tax lots                      | 🟡         |
| Reconciliation                | 🟡         |
| QFX/OFX import                | 🟡         |
| Portfolio performance         | 🟡         |

If the goal were _your own daily-use application_, I would seriously consider it. The most sensible MVP would be **accounts + double-entry transaction ledger + categories + CSV/QFX import + investment transactions + holdings + reconciliation**. Once that foundation is solid, reports and more sophisticated investment analysis become incremental rather than architectural rewrites.

<br>

## Technology

| Layer                | Technology     | Role                                         |
| -------------------- | -------------- | -------------------------------------------- |
| UI                   | **HTML + CSS** | Structure and presentation                   |
| Application/UI logic | **TypeScript** | Interaction, state, domain logic             |
| Desktop framework    | **Tauri**      | Packages the web UI as a native application  |
| Native layer         | **Rust**       | OS integration, native APIs, database access |
| Database             | **SQLite**     | Local persistent financial data              |

So the overall architecture is roughly:

```
┌─────────────────────────────────────────────┐
│                 Tauri App                   │
│                                             │
│  ┌───────────────────────────────────────┐  │
│  │        HTML / CSS / TypeScript        │  │
│  │                                       │  │
│  │  UI                                   │  │
│  │  Application logic                    │  │
│  │  Financial domain model               │  │
│  │  Investment calculations              │  │
│  │  Reporting                            │  │
│  └──────────────────┬────────────────────┘  │
│                     │ Tauri IPC              │
│  ┌──────────────────▼────────────────────┐  │
│  │                 Rust                  │  │
│  │                                       │  │
│  │  Native OS access                     │  │
│  │  Filesystem                           │  │
│  │  SQLite access                        │  │
│  │  Secure storage                       │  │
│  │  Native dialogs                       │  │
│  └──────────────────┬────────────────────┘  │
│                     │                       │
│              ┌──────▼──────┐                │
│              │   SQLite    │                │
│              │             │                │
│              │ Financial   │                │
│              │ data        │                │
│              └─────────────┘                │
└─────────────────────────────────────────────┘
```

### One important distinction

I would **not put everything in Rust just because you're using Tauri**.

I'd make TypeScript responsible for the application's actual intelligence:

- What constitutes a transaction
- Categories
- Splits
- Account balances
- Investment transactions
- Holdings
- Cost basis
- Tax lots
- Portfolio calculations
- Reports
- UI behavior

Rust would primarily be the **native infrastructure layer**:

- Open/read/write files
- SQLite
- OS integration
- Secure credential/key storage
- Native file dialogs
- Potentially background/native operations

That keeps the Rust/TypeScript boundary relatively small.

### And SQLite is particularly appropriate

For a single-user desktop finance application, SQLite is almost ideal. You don't need a database server, the entire database is a file, transactions are ACID, and it can easily handle vastly more records than you'll ever generate in a personal-finance application.

It also gives you a potentially valuable property: **your financial data isn't trapped inside some proprietary application format.** The database is a standard SQLite database that you can back up, inspect, and potentially migrate independently of the application.

### I'd add one more technology

**Git + GitHub**, obviously, for source control.

And I'd probably use **Vite** as the TypeScript/frontend build tool. That would give you something like:

> **TypeScript + HTML + CSS + Vite + Tauri + Rust + SQLite + Git**

You can think of the roles as:

- **HTML** → what appears on the screen
- **CSS** → how it looks
- **TypeScript** → what it does
- **Vite** → develops/builds the frontend
- **Tauri** → turns the frontend into a desktop application and provides the bridge to native functionality
- **Rust** → native functionality
- **SQLite** → persistent data

I wouldn't add React, Svelte, Vue, or another UI framework unless the application's UI eventually demonstrates a real need for one. For the sort of application we're discussing, plain TypeScript is entirely capable of producing a sophisticated UI.

In fact, I think this stack gives you a nice balance: **very little abstraction between you and the machine, but enough structure that the application can grow considerably without becoming unwieldy.**



## Design

* Modular (object-oriented?) to allow for expansion. Every module/object has well-defined API for interaction with other modules/objects

* Create foundational account design - everything else flows from that

* Accuracy and data protection are the highest priorities. Every major update must take those into account.

* Multiple theme selection

* Vary font size 