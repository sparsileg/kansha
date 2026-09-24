//! Synthetic household dataset (TEST-110, D-130).
//!
//! A deterministic, realistic multi-year dataset for the prototype UI,
//! report snapshots, and performance checks (NFR-040, NFR-050): five
//! accounts, about 40 categories, memorized payees, paychecks, bills,
//! card spending with splits and tags, cash, a card payment each month,
//! savings interest, a loan, tithing, and a few future-dated entries. The
//! same seed always produces the same data. No real financial data is ever
//! used before 1.0 (D-130).
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

use chrono::{Datelike, Days};
use rust_decimal::Decimal;

use crate::accounts::{AccountFields, AccountId, AccountType};
use crate::categories::{
    CategoryFields, CategoryId, CategoryKind, PayeeFields, PayeeId, SystemCategory, TagFields,
    TagId,
};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::ledger::{self, Cleared, Entry, EntryLine, Target};
use crate::money::{Money, Rate};
use crate::persistence::{Tx, accounts, categories, payees, tags};

/// What to generate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleSpec {
    /// Same seed, same data.
    pub seed: u64,
    /// First day with transactions (also the opening-balance date).
    pub start: Date,
    /// Last day with transactions; days after `today` are future-dated.
    pub end: Date,
    /// The app's today: entries dated after it are unmarked, older ones
    /// (45+ days) are cleared.
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
];

/// Accounts: (name, type, opening balance in cents, credit limit cents).
const ACCOUNTS: &[(&str, AccountType, i64, Option<i64>)] = &[
    ("Checking", AccountType::Checking, 850_000, None),
    ("Savings", AccountType::Savings, 2_500_000, None),
    ("Visa", AccountType::CreditCard, -120_000, Some(1_500_000)),
    ("Cash", AccountType::Cash, 30_000, None),
    ("Auto Loan", AccountType::Loan, -1_800_000, None),
];

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
    let count = |table: &str| -> Result<u32> {
        let n: i64 = tx
            .conn()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    };
    let len = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    Ok(SampleSummary {
        accounts: len(g.accounts.len()),
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
            entry.cleared = Cleared::Cleared;
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

    /// Whether an entry on `date` counts as cleared: at least 45 days
    /// before today.
    fn cleared(&self, date: Date) -> Cleared {
        let cutoff = self.spec.today.naive() - Days::new(45);
        if date.naive() <= cutoff {
            Cleared::Cleared
        } else {
            Cleared::Unmarked
        }
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
                let pay = grow(265_000);
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
            for (bill_day, payee, cat, cents, card, _) in BILLS {
                if dom == *bill_day {
                    let account = if *card { VISA } else { CHECKING };
                    self.spend(account, date, payee, cat, grow(*cents))?;
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
                self.transfer(CHECKING, SAVINGS, date, "Savings Transfer", 50_000)?;
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
            day = day + Days::new(1);
        }
        Ok(())
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
        assert_eq!(summary.accounts, 5);
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

        // Future-dated entries exist and are unmarked; old ones are cleared.
        let future: i64 = db
            .conn()
            .query_row(
                "SELECT count(*) FROM txn WHERE txn_date > '2026-06-30'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(future > 0);
        let cleared_old: i64 = db
            .conn()
            .query_row(
                "SELECT count(*) FROM posting p JOIN txn t ON t.id = p.txn_id
                 WHERE p.account_id IS NOT NULL AND t.txn_date <= '2026-05-01'
                   AND p.cleared = 'unmarked'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cleared_old, 0);

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
