//! Synthetic household dataset (TEST-110, D-130).
//!
//! A deterministic, realistic multi-year dataset for the prototype UI,
//! report snapshots, and performance checks (NFR-040, NFR-050): five
//! banking accounts, about 40 categories, memorized payees, paychecks,
//! bills, card spending with splits and tags, cash, a card payment each
//! month, savings interest, a loan, tithing, and a few future-dated
//! entries; and (Phase 6) a brokerage and a Roth IRA with five securities,
//! monthly prices, buys, sales, dividends, reinvestments, a split, and
//! opening lots. The same seed always produces the same data. No real
//! financial data is ever used before 1.0 (D-130).
//!
//! Everything goes through the real engine, so the result passes the
//! integrity check. Run it inside one [`Db::write`] so it commits as a
//! whole:
//!
//! ```
//! use kansha_core::sample::{SampleSpec, generate};
//! use kansha_core::{Db, FixedClock, Origin};
//!
//! # fn main() -> kansha_core::Result<()> {
//! let today = "2026-06-30".parse()?;
//! let clock = FixedClock::new(today);
//! let mut db = Db::open_in_memory(&clock)?;
//! let spec = SampleSpec::new(7, "2025-01-01".parse()?, today);
//! let summary = db.write(&clock, Origin::System, |tx| generate(tx, &spec))?;
//! assert!(summary.txns > 500);
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;

use chrono::{Datelike, Days, Months};
use rusqlite::Connection;
use rust_decimal::Decimal;

use crate::accounts::{AccountFields, AccountId, AccountType};
use crate::categories::{
    CategoryFields, CategoryId, CategoryKind, PayeeFields, PayeeId, SystemCategory, TagFields,
    TagId,
};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::invest::{self, InvAction, InvInput, SplitRatio};
use crate::ledger::{self, Cleared, Entry, EntryLine, Target};
use crate::money::{Money, Price, Quantity, Rate, mul_div};
use crate::persistence::{self, Tx, accounts, categories, payees, securities, tags};
use crate::reconcile::{self, Item, StartInput};
use crate::securities::{
    AssetClass, PricePoint, PriceSource, SecurityFields, SecurityId, SecurityType,
};

/// What to generate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleSpec {
    /// Same seed, same data.
    pub seed: u64,
    /// First day with transactions (also the opening-balance date).
    pub start: Date,
    /// Last day with transactions; days after `today` are future-dated.
    pub end: Date,
    /// The app's today. The month before today's is the statement month:
    /// every account that reconciles has a finished statement for each
    /// month end before it, so older entries are reconciled and the rest
    /// are unmarked (RCN-020).
    pub today: Date,
    /// Everyday spending events per day, in percent: 100 is about one a
    /// day. Raise it to build a register of 10,000 rows quickly.
    pub density: u32,
}

impl SampleSpec {
    /// Density 100; the range ends at `today`.
    pub fn new(seed: u64, start: Date, today: Date) -> SampleSpec {
        SampleSpec {
            seed,
            start,
            end: today,
            today,
            density: 100,
        }
    }
}

impl SampleSpec {
    /// The prototype's default: three years back from the first of
    /// today's month, ending three weeks after `today` so the register
    /// shows future-dated entries (REG-070).
    pub fn around(seed: u64, today: Date) -> Result<SampleSpec> {
        let start = Date::from_ymd(today.year() - 3, today.month(), 1)?;
        let end = today
            .naive()
            .checked_add_days(Days::new(21))
            .map(Date::from_naive)
            .ok_or(Error::Overflow("SampleSpec::around"))?;
        let mut spec = SampleSpec::new(seed, start, today);
        spec.end = end;
        Ok(spec)
    }
}

impl SampleSpec {
    /// First day of the statement month: the month before today's.
    pub fn statement_month(&self) -> Result<Date> {
        let today = self.today;
        let (year, month) = match today.month() {
            1 => (today.year() - 1, 12),
            m => (today.year(), m - 1),
        };
        Date::from_ymd(year, month, 1)
    }

    /// Last day of the statement month: the date of the first statement
    /// left to reconcile.
    pub fn statement_date(&self) -> Result<Date> {
        let first = self.statement_month()?.naive();
        first
            .checked_add_months(Months::new(1))
            .and_then(|next| next.pred_opt())
            .map(Date::from_naive)
            .ok_or(Error::Overflow("sample statement date"))
    }
}

/// What was generated.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SampleSummary {
    pub accounts: u32,
    pub categories: u32,
    pub payees: u32,
    pub txns: u32,
}

/// splitmix64: small, fast, and identical on every platform.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n > 0`).
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// Uniform in `lo..=hi`.
    fn between(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1).max(1) as u64) as i64
    }

    /// True with probability `permille` / 1000.
    fn chance(&mut self, permille: u64) -> bool {
        self.below(1000) < permille
    }
}

// ---------------------------------------------------------------------------
// Static data
// ---------------------------------------------------------------------------

/// (path, kind, tax-related, tithable, giving)
const CATEGORIES: &[(&str, CategoryKind, bool, bool, bool)] = &[
    ("Income", CategoryKind::Income, false, false, false),
    ("Income:Salary", CategoryKind::Income, true, true, false),
    ("Income:Refunds", CategoryKind::Income, true, false, false),
    ("Income:Other", CategoryKind::Income, false, false, false),
    ("Housing", CategoryKind::Expense, false, false, false),
    ("Housing:Rent", CategoryKind::Expense, false, false, false),
    (
        "Housing:Repairs",
        CategoryKind::Expense,
        false,
        false,
        false,
    ),
    (
        "Housing:Renters Insurance",
        CategoryKind::Expense,
        false,
        false,
        false,
    ),
    ("Utilities", CategoryKind::Expense, false, false, false),
    (
        "Utilities:Electric",
        CategoryKind::Expense,
        false,
        false,
        false,
    ),
    ("Utilities:Gas", CategoryKind::Expense, false, false, false),
    (
        "Utilities:Water",
        CategoryKind::Expense,
        false,
        false,
        false,
    ),
    (
        "Utilities:Internet",
        CategoryKind::Expense,
        false,
        false,
        false,
    ),
    (
        "Utilities:Phone",
        CategoryKind::Expense,
        false,
        false,
        false,
    ),
    ("Food", CategoryKind::Expense, false, false, false),
    ("Food:Groceries", CategoryKind::Expense, false, false, false),
    ("Food:Dining", CategoryKind::Expense, false, false, false),
    ("Food:Coffee", CategoryKind::Expense, false, false, false),
    ("Transportation", CategoryKind::Expense, false, false, false),
    (
        "Transportation:Fuel",
        CategoryKind::Expense,
        false,
        false,
        false,
    ),
    (
        "Transportation:Auto Insurance",
        CategoryKind::Expense,
        false,
        false,
        false,
    ),
    (
        "Transportation:Maintenance",
        CategoryKind::Expense,
        false,
        false,
        false,
    ),
    ("Health", CategoryKind::Expense, false, false, false),
    ("Health:Doctor", CategoryKind::Expense, true, false, false),
    ("Health:Pharmacy", CategoryKind::Expense, true, false, false),
    (
        "Health:Insurance",
        CategoryKind::Expense,
        true,
        false,
        false,
    ),
    ("Entertainment", CategoryKind::Expense, false, false, false),
    (
        "Entertainment:Streaming",
        CategoryKind::Expense,
        false,
        false,
        false,
    ),
    (
        "Entertainment:Movies",
        CategoryKind::Expense,
        false,
        false,
        false,
    ),
    (
        "Entertainment:Books",
        CategoryKind::Expense,
        false,
        false,
        false,
    ),
    ("Shopping", CategoryKind::Expense, false, false, false),
    (
        "Shopping:Clothing",
        CategoryKind::Expense,
        false,
        false,
        false,
    ),
    (
        "Shopping:Household",
        CategoryKind::Expense,
        false,
        false,
        false,
    ),
    (
        "Shopping:Electronics",
        CategoryKind::Expense,
        false,
        false,
        false,
    ),
    ("Shopping:Gifts", CategoryKind::Expense, false, false, false),
    ("Charity", CategoryKind::Expense, true, false, true),
    ("Charity:Tithe", CategoryKind::Expense, true, false, true),
    (
        "Charity:Other Giving",
        CategoryKind::Expense,
        true,
        false,
        true,
    ),
    ("Taxes", CategoryKind::Expense, true, false, false),
    ("Taxes:Property", CategoryKind::Expense, true, false, false),
    ("Bank Fees", CategoryKind::Expense, false, false, false),
];

const TAG_NAMES: &[&str] = &["Vacation", "Business", "Kids", "Deductible"];

/// Everyday merchants: (name, category, lo cents, hi cents, weight, on card).
type Merchant = (&'static str, &'static str, i64, i64, u32, bool);

const MERCHANTS: &[Merchant] = &[
    ("Safeway", "Food:Groceries", 2500, 14000, 10, true),
    ("Trader Joe's", "Food:Groceries", 2000, 9500, 8, true),
    ("Whole Foods", "Food:Groceries", 3000, 16000, 5, true),
    ("Kroger", "Food:Groceries", 2200, 12000, 6, false),
    ("Aldi", "Food:Groceries", 1800, 8500, 4, false),
    ("Costco", "Food:Groceries", 8000, 26000, 4, true),
    ("Chipotle", "Food:Dining", 1000, 2600, 5, true),
    ("Panera", "Food:Dining", 900, 2400, 4, true),
    ("Local Diner", "Food:Dining", 1500, 4800, 3, true),
    ("Pizza Place", "Food:Dining", 1800, 5200, 3, true),
    ("Thai Kitchen", "Food:Dining", 2200, 6800, 3, true),
    ("Sushi Go", "Food:Dining", 3500, 9000, 2, true),
    ("Starbucks", "Food:Coffee", 450, 1100, 6, true),
    ("Local Coffee", "Food:Coffee", 350, 900, 4, false),
    ("Shell", "Transportation:Fuel", 3200, 6800, 5, true),
    ("Chevron", "Transportation:Fuel", 3000, 6500, 4, true),
    ("Costco Gas", "Transportation:Fuel", 3500, 7000, 3, true),
    (
        "Jiffy Lube",
        "Transportation:Maintenance",
        4500,
        18000,
        1,
        true,
    ),
    ("Amazon", "Shopping:Household", 900, 12000, 8, true),
    ("Target", "Shopping:Household", 1500, 9500, 5, true),
    ("Old Navy", "Shopping:Clothing", 2500, 12000, 2, true),
    ("Best Buy", "Shopping:Electronics", 2000, 45000, 1, true),
    ("Gift Shop", "Shopping:Gifts", 1500, 8000, 1, true),
    ("Home Depot", "Housing:Repairs", 1200, 22000, 2, true),
    ("CVS", "Health:Pharmacy", 800, 6000, 3, true),
    ("Dr. Smith", "Health:Doctor", 2500, 20000, 1, false),
    ("Cinema 12", "Entertainment:Movies", 1400, 4200, 2, true),
    ("Bookstore", "Entertainment:Books", 1200, 4500, 1, true),
    ("Red Cross", "Charity:Other Giving", 2000, 10000, 1, false),
];

/// Bills on a fixed day each month:
/// (day, payee, category, cents, on card, check number).
type Bill = (u32, &'static str, &'static str, i64, bool, bool);

const BILLS: &[Bill] = &[
    (3, "Comcast", "Utilities:Internet", 7999, false, false),
    (5, "Verizon", "Utilities:Phone", 8500, false, false),
    (
        7,
        "State Farm",
        "Transportation:Auto Insurance",
        14250,
        false,
        false,
    ),
    (8, "Blue Cross", "Health:Insurance", 31000, false, false),
    (4, "Netflix", "Entertainment:Streaming", 1549, true, false),
    (6, "Spotify", "Entertainment:Streaming", 1099, true, false),
    (
        10,
        "Renters Cover",
        "Housing:Renters Insurance",
        1800,
        false,
        false,
    ),
    // Paid by check late in the month: outstanding at the statement.
    (27, "Green Lawn Co", "Housing:Repairs", 6500, false, true),
];

/// Accounts: (name, type, opening balance in cents, credit limit cents).
const ACCOUNTS: &[(&str, AccountType, i64, Option<i64>)] = &[
    ("Checking", AccountType::Checking, 850_000, None),
    ("Savings", AccountType::Savings, 2_500_000, None),
    ("Visa", AccountType::CreditCard, -120_000, Some(1_500_000)),
    ("Cash", AccountType::Cash, 30_000, None),
    ("Auto Loan", AccountType::Loan, -1_800_000, None),
];

/// Checking's balance after the monthly sweep on the 25th, in cents,
/// before yearly growth.
const CUSHION: i64 = 400_000;

const CHECKING: usize = 0;
const SAVINGS: usize = 1;
const VISA: usize = 2;
const CASH: usize = 3;
const LOAN: usize = 4;

// ---------------------------------------------------------------------------
// Generator
// ---------------------------------------------------------------------------

struct Gen<'a, 'c> {
    tx: &'a Tx<'c>,
    spec: &'a SampleSpec,
    rng: Rng,
    accounts: Vec<AccountId>,
    cats: HashMap<&'static str, CategoryId>,
    payees: HashMap<&'static str, PayeeId>,
    tags: Vec<TagId>,
    check_num: i64,
}

/// Fill an empty database with the dataset. Refuses a database that
/// already has accounts.
pub fn generate(tx: &Tx<'_>, spec: &SampleSpec) -> Result<SampleSummary> {
    if spec.end < spec.start {
        return Err(Error::Invalid("sample data: end is before start".into()));
    }
    if !accounts::list(tx.conn())?.is_empty() {
        return Err(Error::Invalid(
            "sample data can only be loaded into an empty book".into(),
        ));
    }
    let mut g = Gen {
        tx,
        spec,
        rng: Rng(spec.seed),
        accounts: Vec::new(),
        cats: HashMap::new(),
        payees: HashMap::new(),
        tags: Vec::new(),
        check_num: 1000,
    };
    g.setup()?;
    g.run()?;
    g.reconcile_history()?;
    let investment_accounts = investments(tx, spec)?;
    let count = |table: &str| -> Result<u32> {
        let n: i64 = tx
            .conn()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    };
    let len = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    Ok(SampleSummary {
        accounts: len(g.accounts.len() + investment_accounts),
        categories: len(CATEGORIES.len()),
        payees: len(g.payees.len()),
        txns: count("txn")?.saturating_sub(len(ACCOUNTS.len())),
    })
}

impl Gen<'_, '_> {
    fn setup(&mut self) -> Result<()> {
        let opening = categories::system(self.tx.conn(), SystemCategory::OpeningBalance)?.id;
        for (name, kind, tax, tithable, giving) in CATEGORIES {
            let (parent, leaf) = match name.rsplit_once(':') {
                Some((p, leaf)) => (self.cats.get(p).copied(), leaf),
                None => (None, *name),
            };
            let mut f = CategoryFields::new(leaf, *kind);
            f.parent = parent;
            f.tax_related = *tax;
            f.tithable = *tithable;
            f.giving = *giving;
            let id = categories::insert(self.tx, &f)?.id;
            self.cats.insert(name, id);
        }
        for name in TAG_NAMES {
            self.tags
                .push(tags::insert(self.tx, &TagFields::new(*name))?.id);
        }
        for (name, kind, cents, limit) in ACCOUNTS {
            let mut f = AccountFields::new(*name, *kind);
            f.opening_date = Some(self.spec.start);
            f.credit_limit = limit.map(Money::from_cents);
            if *kind == AccountType::Savings {
                f.interest_rate = Some(Rate::from_raw(4_100_000));
            }
            let id = accounts::insert(self.tx, &f)?.id;
            self.accounts.push(id);
            let amount = Money::from_cents(*cents);
            let mut entry =
                Entry::new(id, self.spec.start, amount).line(Target::Category(opening), amount);
            entry.cleared = self.cleared(self.spec.start);
            ledger::create_entry(self.tx, &entry)?;
        }
        // Memorized payees (PAY-020): merchants and bills know their
        // category; QuickFill uses it.
        for (name, cat, ..) in MERCHANTS {
            self.payee(name, Some(cat))?;
        }
        for (_, name, cat, ..) in BILLS {
            self.payee(name, Some(cat))?;
        }
        for (name, cat) in [
            ("Landlord", "Housing:Rent"),
            ("Employer", "Income:Salary"),
            ("Church", "Charity:Tithe"),
            ("City Power", "Utilities:Electric"),
            ("Gas Co", "Utilities:Gas"),
            ("Water Dept", "Utilities:Water"),
            ("IRS", "Income:Refunds"),
            ("County Treasurer", "Taxes:Property"),
            ("Bank", "Income:Other"),
        ] {
            self.payee(name, Some(cat))?;
        }
        for name in [
            "Visa Payment",
            "ATM",
            "Savings Transfer",
            "Auto Loan Payment",
        ] {
            self.payee(name, None)?;
        }
        Ok(())
    }

    fn payee(&mut self, name: &'static str, category: Option<&str>) -> Result<()> {
        let mut f = PayeeFields::new(name);
        f.default_category = category.and_then(|c| self.cats.get(c).copied());
        let id = payees::insert(self.tx, &f)?.id;
        self.payees.insert(name, id);
        Ok(())
    }

    fn cat(&self, path: &str) -> CategoryId {
        // Every path used below is declared in CATEGORIES; a typo fails
        // the generator's own tests.
        self.cats.get(path).copied().unwrap_or(CategoryId(0))
    }

    fn statement_month(&self) -> Result<Date> {
        self.spec.statement_month()
    }

    /// Entries before the statement month are cleared here and reconciled
    /// by [`Gen::reconcile_history`]; the rest are unmarked.
    fn cleared(&self, date: Date) -> Cleared {
        match self.statement_month() {
            Ok(first) if date < first => Cleared::Cleared,
            _ => Cleared::Unmarked,
        }
    }

    /// One finished statement per month end before the statement month,
    /// for every account that reconciles, each covering every entry up to
    /// its date (RCN-020, RCN-060).
    fn reconcile_history(&mut self) -> Result<()> {
        let first = self.statement_month()?;
        let mut month_ends = Vec::new();
        let mut month = Date::from_ymd(self.spec.start.year(), self.spec.start.month(), 1)?;
        while month < first {
            let next = month
                .naive()
                .checked_add_months(Months::new(1))
                .ok_or(Error::Overflow("sample statement date"))?;
            let end = next
                .pred_opt()
                .ok_or(Error::Overflow("sample statement date"))?;
            month_ends.push(Date::from_naive(end));
            month = Date::from_naive(next);
        }
        for &account in &self.accounts {
            if !reconcile::is_reconcilable(&accounts::get(self.tx.conn(), account)?) {
                continue;
            }
            let sign = reconcile::Sign::of(self.tx.conn(), account)?;
            for &statement_date in &month_ends {
                let rec = reconcile::start(
                    self.tx,
                    &StartInput {
                        account,
                        statement_date,
                        statement_balance: sign.apply(ledger::balance(
                            self.tx.conn(),
                            account,
                            Some(statement_date),
                        )?)?,
                        interest: None,
                        service_charge: None,
                    },
                )?;
                reconcile::finish(self.tx, rec.id)?;
            }
        }
        Ok(())
    }

    fn post(&mut self, mut entry: Entry, payee: &str) -> Result<()> {
        entry.payee = self.payees.get(payee).copied();
        entry.cleared = self.cleared(entry.date);
        for line in &mut entry.lines {
            if matches!(line.target, Target::Account(_)) {
                line.cleared = entry.cleared;
            }
        }
        ledger::create_entry(self.tx, &entry)?;
        Ok(())
    }

    /// A payment or charge of `cents` to `category`.
    fn spend(
        &mut self,
        account: usize,
        date: Date,
        payee: &'static str,
        category: &str,
        cents: i64,
    ) -> Result<()> {
        let amount = Money::from_cents(-cents);
        let entry = Entry::new(self.accounts[account], date, amount)
            .line(Target::Category(self.cat(category)), amount);
        self.post(entry, payee)
    }

    fn transfer(
        &mut self,
        from: usize,
        to: usize,
        date: Date,
        payee: &'static str,
        cents: i64,
    ) -> Result<()> {
        let amount = Money::from_cents(-cents);
        let entry = Entry::new(self.accounts[from], date, amount)
            .line(Target::Account(self.accounts[to]), amount);
        self.post(entry, payee)
    }

    fn run(&mut self) -> Result<()> {
        let spec = self.spec;
        let start_year = spec.start.year();
        let mut tithable: i64 = 0;
        let statement_month = self.statement_month()?;
        let after_statement_month = statement_month
            .naive()
            .checked_add_months(Months::new(1))
            .map(Date::from_naive)
            .ok_or(Error::Overflow("sample statement month"))?;
        let mut day = spec.start.naive();
        let end = spec.end.naive();
        while day <= end {
            let date = Date::from_naive(day);
            let years = i64::from(day.year() - start_year);
            let grow = |cents: i64| cents * (1000 + 30 * years) / 1000;
            let dom = day.day();
            let last_of_month = (day + Days::new(1)).month() != day.month();

            // Income.
            if dom == 1 || dom == 15 {
                let pay = grow(285_000);
                let amount = Money::from_cents(pay);
                let entry = Entry::new(self.accounts[CHECKING], date, amount)
                    .line(Target::Category(self.cat("Income:Salary")), amount);
                self.post(entry, "Employer")?;
                tithable += pay;
            }
            if day.month() == 4 && dom == 15 && years > 0 {
                let amount = Money::from_cents(124_000);
                let entry = Entry::new(self.accounts[CHECKING], date, amount)
                    .line(Target::Category(self.cat("Income:Refunds")), amount);
                self.post(entry, "IRS")?;
            }

            // Fixed bills.
            if dom == 1 {
                self.check_num += 1;
                let cents = grow(185_000);
                let amount = Money::from_cents(-cents);
                let mut entry = Entry::new(self.accounts[CHECKING], date, amount)
                    .line(Target::Category(self.cat("Housing:Rent")), amount);
                entry.check_num = self.check_num.to_string();
                self.post(entry, "Landlord")?;
            }
            for (bill_day, payee, cat, cents, card, by_check) in BILLS {
                if dom == *bill_day {
                    let account = if *card { VISA } else { CHECKING };
                    if *by_check {
                        self.check_num += 1;
                        let amount = Money::from_cents(-grow(*cents));
                        let mut entry = Entry::new(self.accounts[account], date, amount)
                            .line(Target::Category(self.cat(cat)), amount);
                        entry.check_num = self.check_num.to_string();
                        self.post(entry, payee)?;
                    } else {
                        self.spend(account, date, payee, cat, grow(*cents))?;
                    }
                }
            }
            if dom == 9 {
                let cents = self.rng.between(6000, 16000);
                self.spend(CHECKING, date, "City Power", "Utilities:Electric", cents)?;
            }
            if dom == 11 {
                let winter = matches!(day.month(), 11 | 12 | 1 | 2 | 3);
                let cents = if winter {
                    self.rng.between(6000, 14000)
                } else {
                    self.rng.between(2500, 5500)
                };
                self.spend(CHECKING, date, "Gas Co", "Utilities:Gas", cents)?;
            }
            if dom == 12 {
                let cents = self.rng.between(4000, 7000);
                self.spend(CHECKING, date, "Water Dept", "Utilities:Water", cents)?;
            }
            if matches!(day.month(), 4 | 10) && dom == 10 {
                self.spend(
                    CHECKING,
                    date,
                    "County Treasurer",
                    "Taxes:Property",
                    grow(145_000),
                )?;
            }

            // Cash, savings, tithing.
            if dom == 2 {
                let cents = self.rng.between(10_000, 20_000) / 500 * 500;
                self.transfer(CHECKING, CASH, date, "ATM", cents)?;
            }
            if dom == 16 && tithable > 0 {
                let cents = tithable / 10;
                tithable = 0;
                self.check_num += 1;
                let amount = Money::from_cents(-cents);
                let mut entry = Entry::new(self.accounts[CHECKING], date, amount)
                    .line(Target::Category(self.cat("Charity:Tithe")), amount);
                entry.check_num = self.check_num.to_string();
                entry.lines[0].tags = vec![self.tags[3]];
                self.post(entry, "Church")?;
            }
            if dom == 25 {
                // Sweep: keep a cushion in checking so it never runs dry;
                // the excess goes to savings, a shortfall comes back.
                let cushion = grow(CUSHION);
                let balance =
                    ledger::balance(self.tx.conn(), self.accounts[CHECKING], Some(date))?.cents();
                if balance > cushion + 10_000 {
                    let cents = (balance - cushion) / 10_000 * 10_000;
                    self.transfer(CHECKING, SAVINGS, date, "Savings Transfer", cents)?;
                } else if balance < cushion {
                    let cents = (cushion - balance + 9_999) / 10_000 * 10_000;
                    self.transfer(SAVINGS, CHECKING, date, "Savings Transfer", cents)?;
                }
            }
            if dom == 28 {
                let owed =
                    -ledger::balance(self.tx.conn(), self.accounts[LOAN], Some(date))?.cents();
                let pay = owed.min(41_236);
                if pay > 0 {
                    self.transfer(CHECKING, LOAN, date, "Auto Loan Payment", pay)?;
                }
            }
            if dom == 20 {
                let owed =
                    -ledger::balance(self.tx.conn(), self.accounts[VISA], Some(date))?.cents();
                if owed > 0 {
                    self.transfer(CHECKING, VISA, date, "Visa Payment", owed)?;
                }
            }
            if last_of_month {
                let balance = ledger::balance(self.tx.conn(), self.accounts[SAVINGS], Some(date))?;
                // 4.10 % a year, paid monthly, rounded half-even.
                let interest = Money::from_decimal(
                    balance.to_decimal() * Decimal::new(41, 3) / Decimal::from(12),
                )?;
                if interest.cents() > 0 {
                    let entry = Entry::new(self.accounts[SAVINGS], date, interest)
                        .line(Target::Category(self.cat("Income:Other")), interest);
                    self.post(entry, "Bank")?;
                }
            }

            // Everyday spending.
            // Four trials a day at 25 % gives about one event at density
            // 100; more density means more trials.
            let trials = (4 * self.spec.density / 100).max(u32::from(self.spec.density > 0));
            let mut events = 0;
            for _ in 0..trials {
                if self.rng.chance(250) {
                    events += 1;
                }
            }
            for _ in 0..events {
                self.everyday(date, day.month())?;
            }
            // A busy month on the card: extra charges in the statement
            // month, so its Visa statement has plenty to check off.
            if date >= statement_month && date < after_statement_month {
                for _ in 0..3 {
                    if self.rng.chance(400) {
                        self.card_charge(date)?;
                    }
                }
            }
            day = day + Days::new(1);
        }
        Ok(())
    }

    /// A charge on the card at a merchant that takes it.
    fn card_charge(&mut self, date: Date) -> Result<()> {
        let card: Vec<&Merchant> = MERCHANTS.iter().filter(|m| m.5).collect();
        let total: u64 = card.iter().map(|m| u64::from(m.4)).sum();
        let mut pick = self.rng.below(total);
        let mut merchant = card[0];
        for m in card {
            if pick < u64::from(m.4) {
                merchant = m;
                break;
            }
            pick -= u64::from(m.4);
        }
        let (name, cat, lo, hi, ..) = *merchant;
        let cents = self.rng.between(lo, hi);
        self.spend(VISA, date, name, cat, cents)
    }

    fn everyday(&mut self, date: Date, month: u32) -> Result<()> {
        // Occasional cash purchase.
        if self.rng.chance(120) {
            let cents = self.rng.between(300, 2500);
            let (payee, cat) = if self.rng.chance(500) {
                ("Local Coffee", "Food:Coffee")
            } else {
                ("Local Diner", "Food:Dining")
            };
            return self.spend(CASH, date, payee, cat, cents);
        }
        let total: u64 = MERCHANTS.iter().map(|m| u64::from(m.4)).sum();
        let mut pick = self.rng.below(total);
        let mut merchant = &MERCHANTS[0];
        for m in MERCHANTS {
            if pick < u64::from(m.4) {
                merchant = m;
                break;
            }
            pick -= u64::from(m.4);
        }
        let (name, cat, lo, hi, _, card) = *merchant;
        let cents = self.rng.between(lo, hi);
        let account = if card && self.rng.chance(800) {
            VISA
        } else {
            CHECKING
        };

        // Refund now and then.
        if self.rng.chance(15) {
            let amount = Money::from_cents(cents);
            let mut entry = Entry::new(self.accounts[account], date, amount)
                .line(Target::Category(self.cat(cat)), amount);
            entry.memo = "refund".into();
            return self.post(entry, name);
        }

        let amount = Money::from_cents(-cents);
        let mut entry = Entry::new(self.accounts[account], date, amount);
        let big_shop = matches!(name, "Costco" | "Target" | "Amazon" | "Whole Foods");
        if big_shop && cents > 6000 && self.rng.chance(350) {
            // Split: most of it in the merchant's category, the rest in
            // two others.
            let a = -(cents * 6 / 10);
            let b = -(cents * 25 / 100);
            let c = -cents - a - b;
            let other1 = if cat == "Food:Groceries" {
                "Shopping:Household"
            } else {
                "Food:Groceries"
            };
            entry = entry
                .line(Target::Category(self.cat(cat)), Money::from_cents(a))
                .line(Target::Category(self.cat(other1)), Money::from_cents(b))
                .line(
                    Target::Category(self.cat("Health:Pharmacy")),
                    Money::from_cents(c),
                );
        } else {
            let mut line = EntryLine::new(Target::Category(self.cat(cat)), amount);
            if cat == "Food:Dining" && self.rng.chance(60) {
                line.tags = vec![self.tags[1]];
            }
            if month == 7 && self.rng.chance(300) {
                line.tags = vec![self.tags[0]];
            }
            entry.lines.push(line);
        }
        if self.rng.chance(40) {
            entry.memo = "check later".into();
        }
        self.post(entry, name)
    }
}

// ---------------------------------------------------------------------------
// Mock bank statements
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Investments (Phase 6)
// ---------------------------------------------------------------------------

/// A sample security and how its price and dividend behave.
struct SampleSecurity {
    name: &'static str,
    ticker: &'static str,
    kind: SecurityType,
    class: AssetClass,
    /// Opening price, cents.
    cents: i64,
    /// Monthly drift and swing, tenths of a percent.
    drift: i64,
    swing: i64,
    /// Yearly dividend per share, cents.
    dividend: i64,
}

const fn sec(
    name: &'static str,
    ticker: &'static str,
    kind: SecurityType,
    class: AssetClass,
    [cents, drift, swing, dividend]: [i64; 4],
) -> SampleSecurity {
    SampleSecurity {
        name,
        ticker,
        kind,
        class,
        cents,
        drift,
        swing,
        dividend,
    }
}

const SECURITIES: &[SampleSecurity] = &[
    sec(
        "Vanguard Total Stock Market ETF",
        "VTI",
        SecurityType::Etf,
        AssetClass::UsEquity,
        [22_000, 8, 45, 340],
    ),
    sec(
        "Vanguard Total International Stock ETF",
        "VXUS",
        SecurityType::Etf,
        AssetClass::IntlEquity,
        [5_500, 5, 50, 160],
    ),
    sec(
        "Vanguard Total Bond Market ETF",
        "BND",
        SecurityType::Etf,
        AssetClass::Bond,
        [7_200, 1, 12, 240],
    ),
    sec(
        "Apple Inc",
        "AAPL",
        SecurityType::Stock,
        AssetClass::UsEquity,
        [18_000, 12, 80, 100],
    ),
    sec(
        "Vanguard Wellington Fund Investor",
        "VWELX",
        SecurityType::MutualFund,
        AssetClass::UsEquity,
        [4_000, 6, 30, 140],
    ),
];
const VTI: usize = 0;
const VXUS: usize = 1;
const BND: usize = 2;
const AAPL: usize = 3;
const VWELX: usize = 4;

/// Shares in raw units (×10⁻⁶) that `dollars` buys at `price`, rounded
/// down to a thousandth of a share.
fn shares_for(dollars: i64, price: Price) -> Result<Quantity> {
    let raw = mul_div(dollars * 1_000_000, 1_000_000, price.raw().max(1))?;
    Ok(Quantity::from_raw(raw - raw % 1000))
}

struct InvGen<'a, 'c> {
    tx: &'a Tx<'c>,
    rng: Rng,
    securities: Vec<SecurityId>,
    /// Current price of each security.
    prices: Vec<Price>,
}

impl InvGen<'_, '_> {
    fn post(&mut self, input: &InvInput) -> Result<()> {
        invest::create(self.tx, input).map(|_| ())
    }

    fn input(&self, account: AccountId, action: InvAction, date: Date, sec: usize) -> InvInput {
        let mut i = InvInput::new(account, action, date);
        i.security = Some(self.securities[sec]);
        i
    }

    fn buy(
        &mut self,
        account: AccountId,
        date: Date,
        sec: usize,
        shares: Quantity,
        commission: i64,
    ) -> Result<()> {
        let mut i = self.input(account, InvAction::Buy, date, sec);
        i.quantity = Some(shares);
        i.price = Some(self.prices[sec]);
        i.commission = Money::from_cents(commission);
        self.post(&i)
    }

    fn held(&self, account: AccountId, sec: usize, date: Date) -> Result<Quantity> {
        Ok(persistence::invest::open_lots(
            self.tx.conn(),
            Some(account),
            Some(self.securities[sec]),
            date,
        )?
        .iter()
        .map(|l| l.open_quantity)
        .sum())
    }

    /// Quarterly cash dividend on everything held.
    fn dividend(&mut self, account: AccountId, date: Date, sec: usize) -> Result<()> {
        let held = self.held(account, sec, date)?;
        let per_share = Price::from_raw(SECURITIES[sec].dividend * 10_000 / 4);
        let amount = crate::money::extended_value(held, per_share)?;
        if amount.cents() <= 0 {
            return Ok(());
        }
        let mut i = self.input(account, InvAction::Dividend, date, sec);
        i.amount = Some(amount);
        self.post(&i)
    }

    /// Reinvest `per_share` cents a share into more shares.
    fn reinvest(
        &mut self,
        account: AccountId,
        date: Date,
        sec: usize,
        action: InvAction,
        per_share: i64,
    ) -> Result<()> {
        let held = self.held(account, sec, date)?;
        let amount = crate::money::extended_value(held, Price::from_raw(per_share * 10_000))?;
        let shares = mul_div(
            amount.cents(),
            10_i64.pow(10),
            self.prices[sec].raw().max(1),
        )?;
        let shares = Quantity::from_raw(shares - shares % 1000);
        if amount.cents() <= 0 || shares.raw() <= 0 {
            return Ok(());
        }
        let mut i = self.input(account, action, date, sec);
        i.quantity = Some(shares);
        i.price = Some(self.prices[sec]);
        i.amount = Some(amount);
        self.post(&i)
    }

    /// Move every price one month along its random walk; record it.
    fn step_prices(&mut self, date: Date, skip: Option<usize>) -> Result<()> {
        for (k, sec) in SECURITIES.iter().enumerate() {
            let change = sec.drift + self.rng.between(-sec.swing, sec.swing);
            let next = mul_div(self.prices[k].raw(), 1000 + change, 1000)?;
            self.prices[k] = Price::from_raw((next + 5_000) / 10_000 * 10_000);
            if Some(k) == skip {
                continue;
            }
            securities::set_price(
                self.tx,
                &PricePoint {
                    security: self.securities[k],
                    date,
                    price: self.prices[k],
                    source: PriceSource::Manual,
                },
            )?;
        }
        Ok(())
    }
}

/// A brokerage and a Roth IRA with trades, income, and prices from
/// `spec.start` to today (or `spec.end`, if earlier). Returns how many
/// accounts it made.
fn investments(tx: &Tx<'_>, spec: &SampleSpec) -> Result<usize> {
    let opening = categories::system(tx.conn(), SystemCategory::OpeningBalance)?.id;
    let mut g = InvGen {
        tx,
        rng: Rng(spec.seed ^ 0x1D1E_57ED),
        securities: Vec::new(),
        prices: Vec::new(),
    };
    for s in SECURITIES {
        let mut f = SecurityFields::new(s.name, s.kind);
        f.ticker = Some(s.ticker.to_string());
        f.asset_class = s.class;
        g.securities.push(securities::insert(tx, &f)?.id);
        g.prices.push(Price::from_raw(s.cents * 10_000));
    }
    let mut f = AccountFields::new("Brokerage", AccountType::Brokerage);
    f.opening_date = Some(spec.start);
    f.institution = "Vanguard".into();
    let brokerage = accounts::insert(tx, &f)?.id;
    let mut f = AccountFields::new("Roth IRA", AccountType::RothIra);
    f.opening_date = Some(spec.start);
    let roth = accounts::insert(tx, &f)?.id;

    let last = spec.end.min(spec.today);
    let day = |d: chrono::NaiveDate| Date::from_naive(d);
    let start = spec.start.naive();

    // Opening cash and opening lots (held for years already).
    let mut cash = InvInput::new(brokerage, InvAction::CashIn, spec.start);
    cash.amount = Some(Money::from_cents(6_000_000));
    cash.counterpart = Some(Target::Category(opening));
    cash.memo = "Opening Balance".into();
    g.post(&cash)?;
    for (sec, shares, basis, years) in [(VWELX, 800, 2_400_000, 5), (VTI, 50, 750_000, 3)] {
        let mut i = g.input(roth, InvAction::SharesAdded, spec.start, sec);
        i.quantity = Some(Quantity::from_raw(shares * 1_000_000));
        i.amount = Some(Money::from_cents(basis));
        i.acquired = start.checked_sub_months(Months::new(12 * years)).map(day);
        i.memo = "Opening position".into();
        g.post(&i)?;
    }

    // First purchases a few days in.
    let first = day(start + Days::new(5));
    if first <= last {
        for (sec, shares, commission) in [
            (VTI, 100, 0),
            (VXUS, 200, 0),
            (BND, 150, 0),
            (AAPL, 40, 495),
        ] {
            g.buy(
                brokerage,
                first,
                sec,
                Quantity::from_raw(shares * 1_000_000),
                commission,
            )?;
        }
    }

    let mut split_done = false;
    let mut month = start
        .checked_add_months(Months::new(1))
        .and_then(|m| m.with_day(1))
        .ok_or(Error::Overflow("sample month"))?;
    while day(month) <= last {
        let on = |d: u32| month.with_day(d).map(day);
        // Monthly purchase of VTI on the 10th.
        if let Some(d) = on(10).filter(|d| *d <= last) {
            let shares = shares_for(500, g.prices[VTI])?;
            g.buy(brokerage, d, VTI, shares, 0)?;
        }
        // Quarterly income on the 20th.
        if month.month() % 3 == 0 {
            if let Some(d) = on(20).filter(|d| *d <= last) {
                for sec in [VTI, VXUS, BND, AAPL] {
                    g.dividend(brokerage, d, sec)?;
                }
                g.dividend(roth, d, VTI)?;
                g.reinvest(
                    roth,
                    d,
                    VWELX,
                    InvAction::ReinvestDividend,
                    SECURITIES[VWELX].dividend / 4,
                )?;
            }
        }
        // Year-end capital gain distribution, reinvested.
        if month.month() == 12 {
            if let Some(d) = on(18).filter(|d| *d <= last) {
                g.reinvest(roth, d, VWELX, InvAction::ReinvestCgLong, 120)?;
            }
        }
        // Each January after the first year, sell a fifth of the Apple
        // shares (FIFO), and some international shares by specific lot.
        if month.month() == 1 && month.year() > spec.start.year() {
            if let Some(d) = on(15).filter(|d| *d <= last) {
                let held = g.held(brokerage, AAPL, d)?;
                let fifth = Quantity::from_raw(held.raw() / 5 - (held.raw() / 5) % 1_000_000);
                if fifth.raw() > 0 {
                    let mut i = g.input(brokerage, InvAction::Sell, d, AAPL);
                    i.quantity = Some(fifth);
                    i.price = Some(g.prices[AAPL]);
                    i.commission = Money::from_cents(495);
                    g.post(&i)?;
                }
                let lots = persistence::invest::open_lots(
                    tx.conn(),
                    Some(brokerage),
                    Some(g.securities[VXUS]),
                    d,
                )?;
                if let Some(lot) = lots.first() {
                    let take = Quantity::from_raw(lot.open_quantity.raw().min(25_000_000));
                    let mut i = g.input(brokerage, InvAction::Sell, d, VXUS);
                    i.quantity = Some(take);
                    i.price = Some(g.prices[VXUS]);
                    i.lots = vec![invest::LotPick {
                        lot: lot.lot.id,
                        quantity: take,
                    }];
                    g.post(&i)?;
                }
            }
        }
        // Apple splits 4-for-1 the first August.
        if month.month() == 8 && !split_done {
            if let Some(d) = on(28).filter(|d| *d <= last) {
                let mut i = g.input(brokerage, InvAction::Split, d, AAPL);
                i.split = Some(SplitRatio { new: 4, old: 1 });
                g.post(&i)?;
                g.prices[AAPL] = Price::from_raw(g.prices[AAPL].raw() / 4);
                split_done = true;
            }
        }
        // Month-end: interest on cash, then new prices.
        let end = month
            .checked_add_months(Months::new(1))
            .and_then(|m| m.pred_opt())
            .ok_or(Error::Overflow("sample month"))?;
        if day(end) <= last {
            let balance = persistence::invest::cash_balance(tx.conn(), brokerage, Some(day(end)))?;
            let interest = mul_div(balance.cents(), 25, 12_000)?;
            if interest > 0 {
                let mut i = InvInput::new(brokerage, InvAction::Interest, day(end));
                i.amount = Some(Money::from_cents(interest));
                g.post(&i)?;
            }
            if day(end) < last {
                g.step_prices(day(end), None)?;
            }
        }
        month = month
            .checked_add_months(Months::new(1))
            .ok_or(Error::Overflow("sample month"))?;
    }
    // Prices on the last day; the bond fund's stays three weeks old, so
    // the Holdings tab shows a stale price.
    if last > spec.start {
        g.step_prices(last, Some(BND))?;
    }
    Ok(2)
}

/// A made-up bank statement for a sample account, to reconcile against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    pub account: AccountId,
    pub statement_date: Date,
    /// The last finished statement's ending balance (RCN-030). Amounts
    /// and balances are in statement sign, as the reconcile engine takes
    /// them: a credit card balance owed is positive.
    pub opening_balance: Money,
    pub ending_balance: Money,
    /// Items the bank posted by the statement date, oldest first.
    pub posted: Vec<Item>,
    /// Items in the register on or before the statement date that the
    /// bank has not posted yet.
    pub outstanding: Vec<Item>,
}

/// Days the bank takes to post an item: a check about a week, the bank's
/// own entries (interest) at once, anything else two days.
fn posting_lag(item: &Item) -> u64 {
    if !item.check_num.is_empty() {
        7
    } else if item.payee_name == "Bank" {
        0
    } else {
        2
    }
}

/// The bank's statement for `account` as of `statement_date`: every item
/// not yet reconciled that the bank has posted by then, and the balance
/// they bring it to. Items dated in the last days before the statement are
/// outstanding, as are checks written in its last week.
pub fn statement(conn: &Connection, account: AccountId, statement_date: Date) -> Result<Statement> {
    let sign = reconcile::Sign::of(conn, account)?;
    let opening_balance = reconcile::opening_check(conn, account)?.expected;
    let mut posted = Vec::new();
    let mut outstanding = Vec::new();
    let mut ending = opening_balance;
    for item in persistence::reconcile::open_items(conn, account, statement_date)? {
        let item = sign.item(item)?;
        let posts_on = item
            .date
            .naive()
            .checked_add_days(Days::new(posting_lag(&item)))
            .ok_or(Error::Overflow("sample statement"))?;
        if posts_on <= statement_date.naive() {
            ending = ending
                .checked_add(item.amount)
                .ok_or(Error::Overflow("sample statement"))?;
            posted.push(item);
        } else {
            outstanding.push(item);
        }
    }
    Ok(Statement {
        account,
        statement_date,
        opening_balance,
        ending_balance: ending,
        posted,
        outstanding,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::Origin;
    use crate::{Db, FixedClock, integrity};

    fn load(seed: u64, start: &str, end: &str, today: &str, density: u32) -> (Db, SampleSummary) {
        let clock = FixedClock::new(today.parse().unwrap());
        let mut db = Db::open_in_memory(&clock).unwrap();
        let spec = SampleSpec {
            seed,
            start: start.parse().unwrap(),
            end: end.parse().unwrap(),
            today: today.parse().unwrap(),
            density,
        };
        let summary = db
            .write(&clock, Origin::System, |tx| generate(tx, &spec))
            .unwrap();
        (db, summary)
    }

    #[test]
    fn every_used_category_is_declared() {
        let all: Vec<&str> = CATEGORIES.iter().map(|c| c.0).collect();
        for (_, cat, ..) in MERCHANTS {
            assert!(all.contains(cat), "{cat}");
        }
        for (_, _, cat, ..) in BILLS {
            assert!(all.contains(cat), "{cat}");
        }
        for extra in [
            "Housing:Rent",
            "Income:Salary",
            "Income:Refunds",
            "Income:Other",
            "Charity:Tithe",
            "Utilities:Electric",
            "Utilities:Gas",
            "Utilities:Water",
            "Taxes:Property",
            "Shopping:Household",
            "Food:Groceries",
            "Health:Pharmacy",
            "Food:Coffee",
            "Food:Dining",
        ] {
            assert!(all.contains(&extra), "{extra}");
        }
    }

    #[test]
    fn dataset_is_realistic_deterministic_and_consistent() {
        let (db, summary) = load(7, "2024-01-01", "2026-07-21", "2026-06-30", 100);
        assert_eq!(summary.accounts, 7);
        assert_eq!(summary.categories, 41);
        assert!(summary.payees > 40);
        assert!(summary.txns > 1500, "{summary:?}");
        assert!(integrity::check(db.conn()).unwrap().is_clean());

        // Same seed, same data; different seed, different data.
        let fingerprint = |db: &Db| -> String {
            db.conn()
                .query_row(
                    "SELECT count(*) || ':' || sum(amount) || ':' || sum(id * amount % 9973)
                     FROM posting",
                    [],
                    |r| r.get(0),
                )
                .unwrap()
        };
        let (again, _) = load(7, "2024-01-01", "2026-07-21", "2026-06-30", 100);
        assert_eq!(fingerprint(&db), fingerprint(&again));
        let (other, _) = load(8, "2024-01-01", "2026-07-21", "2026-06-30", 100);
        assert_ne!(fingerprint(&db), fingerprint(&other));

        // Future-dated entries exist. Entries before the statement month
        // (May 2026) are reconciled; the rest are unmarked.
        let future: i64 = db
            .conn()
            .query_row(
                "SELECT count(*) FROM txn WHERE txn_date > '2026-06-30'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(future > 0);
        let status = |sql: &str| -> i64 {
            db.conn()
                .query_row(
                    &format!(
                        "SELECT count(*) FROM posting p JOIN txn t ON t.id = p.txn_id
                         JOIN account a ON a.id = p.account_id WHERE {sql}"
                    ),
                    [],
                    |r| r.get(0),
                )
                .unwrap()
        };
        let reconciled = "p.cleared = 'reconciled' AND p.reconciliation_id IS NOT NULL";
        assert!(status(reconciled) > 1000);
        assert_eq!(
            status("t.txn_date >= '2026-05-01' AND p.cleared <> 'unmarked'"),
            0
        );
        assert_eq!(
            status(&format!(
                "t.txn_date < '2026-05-01' AND a.type NOT IN ('loan', 'brokerage', 'roth_ira')
                 AND NOT ({reconciled})"
            )),
            0
        );
        // The loan does not reconcile (RCN-010); its old entries stay cleared.
        assert_eq!(status("p.cleared = 'cleared' AND a.type <> 'loan'"), 0);

        // Splits and tags appear.
        let splits: i64 = db
            .conn()
            .query_row(
                "SELECT count(*) FROM (SELECT txn_id FROM posting GROUP BY txn_id HAVING count(*) > 2)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(splits > 10, "{splits}");
        let tagged: i64 = db
            .conn()
            .query_row("SELECT count(*) FROM posting_tag", [], |r| r.get(0))
            .unwrap();
        assert!(tagged > 10, "{tagged}");

        // Investments: trades, income, a split, sales with gains, prices.
        let count = |sql: &str| -> i64 { db.conn().query_row(sql, [], |r| r.get(0)).unwrap() };
        assert!(count("SELECT count(*) FROM investment_txn") > 100);
        assert_eq!(
            count("SELECT count(*) FROM investment_txn WHERE action = 'split'"),
            1
        );
        assert!(count("SELECT count(*) FROM lot_disposal WHERE kind = 'sale'") >= 4);
        assert!(count("SELECT count(*) FROM price") > 100);
        let brokerage = accounts::list(db.conn())
            .unwrap()
            .into_iter()
            .find(|a| a.fields.name == "Brokerage")
            .unwrap()
            .id;
        let h = crate::invest::holdings(db.conn(), brokerage, "2026-06-30".parse().unwrap(), None)
            .unwrap();
        assert_eq!(h.positions.len(), 4);
        assert!(!h.missing_prices);
        assert!(h.stale_prices);
        assert!(!h.cash.unwrap().is_negative());
    }

    #[test]
    fn checking_stays_positive_and_the_card_is_busy_in_the_statement_month() {
        let today = "2026-09-24";
        let (db, _) = load(1, "2023-09-01", "2026-10-15", today, 100);
        let all = accounts::list(db.conn()).unwrap();
        let id = |name: &str| all.iter().find(|a| a.fields.name == name).unwrap().id;
        let (checking, visa) = (id("Checking"), id("Visa"));

        let mut day = "2023-09-01".parse::<Date>().unwrap().naive();
        let today = today.parse::<Date>().unwrap().naive();
        while day <= today {
            let balance =
                ledger::balance(db.conn(), checking, Some(Date::from_naive(day))).unwrap();
            assert!(!balance.is_negative(), "checking is {balance} on {day}");
            day = day + Days::new(1);
        }

        let charges = |from: &str, to: &str| -> i64 {
            db.conn()
                .query_row(
                    "SELECT count(*) FROM posting p JOIN txn t ON t.id = p.txn_id
                     WHERE p.account_id = ?1 AND p.amount < 0
                       AND t.txn_date BETWEEN ?2 AND ?3",
                    rusqlite::params![visa, from, to],
                    |r| r.get(0),
                )
                .unwrap()
        };
        let july = charges("2026-07-01", "2026-07-31");
        let august = charges("2026-08-01", "2026-08-31");
        assert!(august >= 2 * july, "July {july}, August {august}");
    }

    #[test]
    fn refuses_a_book_that_already_has_accounts() {
        let clock = FixedClock::new("2026-06-30".parse().unwrap());
        let mut db = Db::open_in_memory(&clock).unwrap();
        let spec = SampleSpec::new(
            1,
            "2026-01-01".parse().unwrap(),
            "2026-03-01".parse().unwrap(),
        );
        db.write(&clock, Origin::System, |tx| generate(tx, &spec))
            .unwrap();
        let err = db
            .write(&clock, Origin::System, |tx| generate(tx, &spec))
            .unwrap_err();
        assert!(err.to_string().contains("empty book"), "{err}");
    }
}
