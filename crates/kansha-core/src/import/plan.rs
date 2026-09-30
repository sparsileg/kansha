//! From QIF records and the mapping to what an import will write. Reads
//! the book (to map names) but writes nothing.
//!
//! - **Accounts** (MIG-060): each QIF account maps to an existing account,
//!   a new one, or is skipped. Default: the book's account of that name,
//!   else a new account if the file has transactions for it, else skip.
//!   Transfers to a skipped account, and an account's transfers to
//!   itself (Quicken's opening balance), go to Opening Balance.
//! - **Transfers** (MIG-070): a transfer exported from both sides (same
//!   date, opposite amounts, each naming the other account) becomes one
//!   transaction. The side with more lines keeps it (a split), else the
//!   first seen; an investment record always keeps it, since only the
//!   investments engine posts to an investment account. The dropped
//!   side's cleared status goes onto the kept transaction's line. A
//!   banking transfer into an investment account with no investment-side
//!   record becomes a Cash In or Cash Out there. Cash moved between two
//!   investment accounts has no single-transaction form: each side is
//!   recorded against Opening Balance, with a warning.
//! - **Investment actions**: `Buy`/`Sell`/`Div`/… map to the engine's
//!   actions; an `X` action (`BuyX`, `DivX`, …) adds the cash transfer as
//!   a Cash In before it (buys, expenses) or a Cash Out after it (sales,
//!   income). `ReinvInt` is Interest then Buy; `StkSplit`'s shares are
//!   new per 10 old; `ShrsIn`/`ShrsOut` are Shares Added/Removed.
//! - **Categories, tags, securities**: only what the imported records use,
//!   plus any the user keeps (A5). Uncategorized lines go to
//!   `Uncategorized`.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use super::qif::{self, AccountKind, BankRecord, InvRecord, QifFile, Target as QifTarget};
use super::{
    AccountChoice, AccountPreview, CategoryChoice, CategoryPreview, ImportNote, ImportOptions,
    ImportPreview, SecurityChoice, SecurityPreview, TagPreview,
};
use crate::accounts::{AccountStatus, AccountType};
use crate::categories::{CategoryId, CategoryKind, SystemCategory, TagId};
use crate::date::Date;
use crate::error::Result;
use crate::invest::{InvAction, SplitRatio};
use crate::ledger::Cleared;
use crate::money::{Money, Price, Quantity, extended_value};
use crate::persistence::{accounts, categories, payees, securities, tags};
use crate::securities::SecurityType;

pub(crate) struct Plan {
    pub accounts: Vec<PlanAccount>,
    pub categories: Vec<PlanCategory>,
    pub tags: Vec<PlanTag>,
    pub securities: Vec<PlanSecurity>,
    /// In the order they are written: date, then file order.
    pub items: Vec<Item>,
    pub prices: Vec<PlanPrice>,
    pub new_payees: i64,
    pub transfers_matched: i64,
    pub warnings: Vec<ImportNote>,
    pub errors: Vec<ImportNote>,
}

pub(crate) struct PlanAccount {
    pub name: String,
    pub qif_type: String,
    pub kind: Option<AccountKind>,
    pub defined: bool,
    pub description: String,
    pub credit_limit: Option<Money>,
    pub records: i64,
    pub first: Option<Date>,
    pub last: Option<Date>,
    pub total: Money,
    pub choice: AccountChoice,
    pub default_type: AccountType,
    /// The Kansha account it becomes is an investment account.
    pub investment_target: bool,
}

pub(crate) struct PlanCategory {
    pub name: String,
    pub listed: bool,
    pub income: Option<bool>,
    pub tax_related: bool,
    pub used: i64,
    /// Σ postings: spending positive, income negative.
    pub total: Money,
    pub choice: CategoryChoice,
    pub imported: bool,
}

pub(crate) struct PlanTag {
    pub name: String,
    pub used: i64,
    pub existing: Option<TagId>,
    pub imported: bool,
}

pub(crate) struct PlanSecurity {
    pub name: String,
    pub symbol: Option<String>,
    pub qif_type: String,
    pub used: i64,
    pub prices: i64,
    pub choice: SecurityChoice,
    pub imported: bool,
}

pub(crate) struct PlanPrice {
    pub security: usize,
    pub date: Date,
    pub price: Price,
}

pub(crate) enum Item {
    Bank(BankItem),
    Inv(InvItem),
}

impl Item {
    pub fn line(&self) -> usize {
        match self {
            Item::Bank(b) => b.line,
            Item::Inv(i) => i.line,
        }
    }

    pub fn account(&self) -> usize {
        match self {
            Item::Bank(b) => b.account,
            Item::Inv(i) => i.account,
        }
    }

    fn date(&self) -> Date {
        match self {
            Item::Bank(b) => b.date,
            Item::Inv(i) => i.date,
        }
    }

    fn order(&self) -> (usize, usize) {
        match self {
            Item::Bank(b) => b.order,
            Item::Inv(i) => i.order,
        }
    }

    /// Takes shares out: goes after the day's other records, since
    /// Quicken holds shares by the day and may write a sale before the
    /// same day's reinvestment.
    fn takes_shares(&self) -> bool {
        matches!(
            self,
            Item::Inv(InvItem {
                action: InvAction::Sell | InvAction::SharesRemoved,
                ..
            })
        )
    }
}

pub(crate) struct BankItem {
    pub line: usize,
    order: (usize, usize),
    pub account: usize,
    pub date: Date,
    pub payee: String,
    pub num: String,
    pub memo: String,
    pub cleared: Cleared,
    pub amount: Money,
    pub void: bool,
    pub tags: Vec<usize>,
    pub lines: Vec<ItemLine>,
}

pub(crate) struct ItemLine {
    pub target: LineTarget,
    /// Same sign as the entry's amount.
    pub amount: Money,
    pub memo: String,
    /// The other account's cleared status (transfers).
    pub cleared: Cleared,
    pub tags: Vec<usize>,
    dropped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineTarget {
    Category(usize),
    Account(usize),
    /// The built-in Opening Balance category.
    Equity,
}

pub(crate) struct InvItem {
    pub line: usize,
    order: (usize, usize),
    pub account: usize,
    pub date: Date,
    pub action: InvAction,
    pub security: Option<usize>,
    pub quantity: Option<Quantity>,
    pub price: Option<Price>,
    pub commission: Money,
    pub amount: Option<Money>,
    pub split: Option<SplitRatio>,
    pub counterpart: Option<Counter>,
    pub memo: String,
    /// This account's cash posting.
    pub cleared: Cleared,
    /// The counterpart account's posting (Cash In/Out from a bank).
    pub counter_cleared: Cleared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Counter {
    Category(usize),
    System(SystemCategory),
    Account(usize),
    Equity,
}

/// Name of the category uncategorized lines go to.
const UNCATEGORIZED: &str = "Uncategorized";

pub(crate) fn build(conn: &Connection, file: &QifFile, options: &ImportOptions) -> Result<Plan> {
    let mut b = Builder {
        conn,
        options,
        accounts: file
            .accounts
            .iter()
            .map(|a| PlanAccount {
                name: a.name.clone(),
                qif_type: a.qif_type.clone(),
                kind: a.kind,
                defined: a.defined,
                description: a.description.clone(),
                credit_limit: a.credit_limit,
                records: 0,
                first: None,
                last: None,
                total: Money::ZERO,
                choice: AccountChoice::Skip,
                default_type: AccountType::Checking,
                investment_target: false,
            })
            .collect(),
        categories: Vec::new(),
        tags: Vec::new(),
        securities: Vec::new(),
        items: Vec::new(),
        record_errors: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
        transfers_matched: 0,
    };
    for c in &file.categories {
        let ix = b.category(&c.name);
        let pc = &mut b.categories[ix];
        pc.listed = true;
        pc.income = c.income;
        pc.tax_related = c.tax_related;
    }
    for t in &file.tags {
        b.tag(&t.name);
    }
    for s in &file.securities {
        let ix = b.security(&s.name);
        let ps = &mut b.securities[ix];
        ps.symbol.clone_from(&s.symbol);
        ps.qif_type.clone_from(&s.qif_type);
    }
    for note in &file.notes {
        b.warnings.push(ImportNote {
            line: Some(line_no(note.line)),
            account: String::new(),
            message: note.message.clone(),
        });
    }
    b.convert(file);
    b.count_records(file);
    b.choose_accounts()?;
    b.drop_skipped();
    b.redirect_transfers();
    b.match_transfers();
    b.compact();
    b.items
        .sort_by_key(|i| (i.date(), i.takes_shares(), i.order()));
    b.usage();
    b.choose_categories()?;
    b.choose_securities()?;
    b.choose_tags()?;
    let prices = b.prices(file);
    let new_payees = b.new_payees()?;
    b.totals();
    Ok(Plan {
        accounts: b.accounts,
        categories: b.categories,
        tags: b.tags,
        securities: b.securities,
        items: b.items,
        prices,
        new_payees,
        transfers_matched: b.transfers_matched,
        warnings: b.warnings,
        errors: b.errors,
    })
}

fn line_no(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

fn count(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

fn abs(m: Money) -> Money {
    m.checked_neg().map_or(m, |n| n.max(m))
}

/// `payee — memo`, whichever there are (investment transactions have no
/// payee of their own).
fn memo_with_payee(payee: &str, memo: &str) -> String {
    match (payee.trim(), memo.trim()) {
        ("", m) => m.to_string(),
        (p, "") => p.to_string(),
        (p, m) => format!("{p} — {m}"),
    }
}

/// The account type a new account gets from its QIF type.
fn default_type(kind: Option<AccountKind>, qif_type: &str) -> AccountType {
    match kind {
        Some(AccountKind::Bank) | None => AccountType::Checking,
        Some(AccountKind::Cash) => AccountType::Cash,
        Some(AccountKind::CreditCard) => AccountType::CreditCard,
        Some(AccountKind::Investment) => {
            if qif_type.to_ascii_lowercase().starts_with("401") {
                AccountType::Retirement401k
            } else {
                AccountType::Brokerage
            }
        }
        Some(AccountKind::OtherAsset) => AccountType::OtherAsset,
        Some(AccountKind::OtherLiability) => AccountType::OtherLiability,
    }
}

fn security_type(qif_type: &str) -> SecurityType {
    match qif_type.trim().to_ascii_lowercase().as_str() {
        "stock" => SecurityType::Stock,
        "mutual fund" => SecurityType::MutualFund,
        "bond" | "u.s. savings bond" => SecurityType::Bond,
        "cd" => SecurityType::Cd,
        "etf" => SecurityType::Etf,
        "money market" | "money market fund" => SecurityType::MoneyMarket,
        _ => SecurityType::Other,
    }
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

/// A leg of a transfer: which record, which line, from whom to whom.
struct Leg {
    item: usize,
    /// The line of a banking record; `None` for an investment record.
    line: Option<usize>,
    from: usize,
    to: usize,
    date: Date,
    /// Change to `from`'s balance.
    amount: Money,
}

struct Builder<'a> {
    conn: &'a Connection,
    options: &'a ImportOptions,
    accounts: Vec<PlanAccount>,
    categories: Vec<PlanCategory>,
    tags: Vec<PlanTag>,
    securities: Vec<PlanSecurity>,
    items: Vec<Item>,
    /// Records that could not be read, by the account they belong to.
    record_errors: Vec<(usize, ImportNote)>,
    warnings: Vec<ImportNote>,
    errors: Vec<ImportNote>,
    transfers_matched: i64,
}

impl Builder<'_> {
    fn category(&mut self, name: &str) -> usize {
        let name = name.trim();
        if let Some(i) = self
            .categories
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(name))
        {
            return i;
        }
        self.categories.push(PlanCategory {
            name: name.to_string(),
            listed: false,
            income: None,
            tax_related: false,
            used: 0,
            total: Money::ZERO,
            choice: CategoryChoice::Create {
                path: String::new(),
                category_kind: CategoryKind::Expense,
            },
            imported: false,
        });
        self.categories.len() - 1
    }

    fn tag(&mut self, name: &str) -> usize {
        let name = name.trim();
        if let Some(i) = self
            .tags
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(name))
        {
            return i;
        }
        self.tags.push(PlanTag {
            name: name.to_string(),
            used: 0,
            existing: None,
            imported: false,
        });
        self.tags.len() - 1
    }

    fn security(&mut self, name: &str) -> usize {
        let name = name.trim();
        if let Some(i) = self
            .securities
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(name))
        {
            return i;
        }
        self.securities.push(PlanSecurity {
            name: name.to_string(),
            symbol: None,
            qif_type: String::new(),
            used: 0,
            prices: 0,
            choice: SecurityChoice::Create {
                name: name.to_string(),
                ticker: None,
                security_type: SecurityType::Other,
            },
            imported: false,
        });
        self.securities.len() - 1
    }

    /// The account named by a transfer, added if the file has no record
    /// of it.
    fn account_ref(&mut self, name: &str) -> usize {
        let name = name.trim();
        if let Some(i) = self
            .accounts
            .iter()
            .position(|a| a.name.eq_ignore_ascii_case(name))
        {
            return i;
        }
        self.accounts.push(PlanAccount {
            name: name.to_string(),
            qif_type: String::new(),
            kind: None,
            defined: false,
            description: String::new(),
            credit_limit: None,
            records: 0,
            first: None,
            last: None,
            total: Money::ZERO,
            choice: AccountChoice::Skip,
            default_type: AccountType::Checking,
            investment_target: false,
        });
        self.accounts.len() - 1
    }

    fn note(&self, line: usize, account: usize, message: impl Into<String>) -> ImportNote {
        ImportNote {
            line: Some(line_no(line)),
            account: self
                .accounts
                .get(account)
                .map(|a| a.name.clone())
                .unwrap_or_default(),
            message: message.into(),
        }
    }

    // -----------------------------------------------------------------
    // Records to items
    // -----------------------------------------------------------------

    fn convert(&mut self, file: &QifFile) {
        let mut seq = 0;
        for r in &file.bank {
            seq += 1;
            match self.bank_item(r, seq) {
                Ok(item) => self.items.push(Item::Bank(item)),
                Err(e) => {
                    let n = self.note(r.line, r.account, e);
                    self.record_errors.push((r.account, n));
                }
            }
        }
        for r in &file.invest {
            seq += 1;
            match self.inv_items(r, seq) {
                Ok((items, warnings)) => {
                    for w in warnings {
                        let n = self.note(r.line, r.account, w);
                        self.warnings.push(n);
                    }
                    self.items.extend(items.into_iter().map(Item::Inv));
                }
                Err(e) => {
                    let n = self.note(r.line, r.account, e);
                    self.record_errors.push((r.account, n));
                }
            }
        }
    }

    fn line_target(&mut self, t: QifTarget) -> Option<LineTarget> {
        match t {
            QifTarget::None => None,
            QifTarget::Category(n) => Some(LineTarget::Category(self.category(&n))),
            QifTarget::Transfer(n) => Some(LineTarget::Account(self.account_ref(&n))),
        }
    }

    fn bank_item(&mut self, r: &BankRecord, seq: usize) -> std::result::Result<BankItem, String> {
        if !r.problems.is_empty() {
            return Err(r.problems.join("; "));
        }
        let date = r.date.ok_or("no date")?;
        let mut lines = Vec::new();
        let mut tags = Vec::new();
        let amount;
        if r.splits.is_empty() {
            amount = r.amount.unwrap_or(Money::ZERO);
            let (t, t_tags) = qif::split_target(&r.category);
            tags = t_tags.iter().map(|t| self.tag(t)).collect();
            let target = match self.line_target(t) {
                Some(t) => Some(t),
                None if amount.is_zero() => None,
                None => Some(LineTarget::Category(self.category(""))),
            };
            if let Some(target) = target {
                lines.push(ItemLine {
                    target,
                    amount,
                    memo: String::new(),
                    cleared: Cleared::Unmarked,
                    tags: Vec::new(),
                    dropped: false,
                });
            }
        } else {
            let mut sum = Money::ZERO;
            for s in &r.splits {
                let a = s.amount.unwrap_or(Money::ZERO);
                sum = sum.checked_add(a).ok_or("amounts too large")?;
                let (t, t_tags) = qif::split_target(&s.category);
                let target = match self.line_target(t) {
                    Some(t) => t,
                    None => LineTarget::Category(self.category("")),
                };
                lines.push(ItemLine {
                    target,
                    amount: a,
                    memo: s.memo.clone(),
                    cleared: Cleared::Unmarked,
                    tags: t_tags.iter().map(|t| self.tag(t)).collect(),
                    dropped: false,
                });
            }
            amount = r.amount.unwrap_or(sum);
            if sum != amount {
                return Err(format!(
                    "the split lines add up to {sum}, not the amount {amount}"
                ));
            }
        }
        Ok(BankItem {
            line: r.line,
            order: (seq, 0),
            account: r.account,
            date,
            payee: r.payee.trim().to_string(),
            num: r.num.trim().to_string(),
            memo: r.memo.trim().to_string(),
            cleared: r.cleared,
            amount,
            void: r.void,
            tags,
            lines,
        })
    }

    /// An investment record as one or more engine transactions, with
    /// warnings.
    fn inv_items(
        &mut self,
        r: &InvRecord,
        seq: usize,
    ) -> std::result::Result<(Vec<InvItem>, Vec<String>), String> {
        if !r.problems.is_empty() {
            return Err(r.problems.join("; "));
        }
        let date = r.date.ok_or("no date")?;
        let act = r.action.trim().to_ascii_lowercase();
        let security = (!r.security.trim().is_empty()).then(|| self.security(&r.security));
        let (target, _) = qif::split_target(&r.category);
        let (xfer, cat) = match target {
            QifTarget::Transfer(n) => (Some(self.account_ref(&n)), None),
            QifTarget::Category(n) => (None, Some(self.category(&n))),
            QifTarget::None => (None, None),
        };
        let amount = r.amount.map(abs);
        let moved = r.transfer_amount.map(abs).or(amount);
        let memo = memo_with_payee(&r.payee, &r.memo);
        let mut warnings = Vec::new();
        let mut sub = 0;
        let mut item = |action: InvAction| {
            sub += 1;
            InvItem {
                line: r.line,
                order: (seq, sub),
                account: r.account,
                date,
                action,
                security: None,
                quantity: None,
                price: None,
                commission: Money::ZERO,
                amount: None,
                split: None,
                counterpart: None,
                memo: memo.clone(),
                cleared: r.cleared,
                counter_cleared: Cleared::Unmarked,
            }
        };
        let need_security = || security.ok_or_else(|| format!("{} needs a security", r.action));
        let need_amount = || amount.ok_or_else(|| format!("{} needs an amount", r.action));
        let need_shares = || {
            r.quantity
                .ok_or_else(|| format!("{} needs a number of shares", r.action))
        };

        // (item, whether an `X` transfer comes before it)
        let (main, x_before): (Vec<InvItem>, Option<bool>) = match act.as_str() {
            "buy" | "buyx" => {
                let mut i = item(InvAction::Buy);
                i.security = Some(need_security()?);
                i.quantity = Some(need_shares()?);
                i.price = r.price;
                i.commission = r.commission.map_or(Money::ZERO, abs);
                i.amount = amount;
                (vec![i], Some(true))
            }
            "sell" | "sellx" => {
                let mut i = item(InvAction::Sell);
                i.security = Some(need_security()?);
                i.quantity = Some(need_shares()?);
                i.price = r.price;
                i.commission = r.commission.map_or(Money::ZERO, abs);
                i.amount = amount;
                (vec![i], Some(false))
            }
            "div" | "divx" | "cglong" | "cglongx" | "cgmid" | "cgmidx" | "cgshort" | "cgshortx" => {
                let (action, system) = match act.trim_end_matches('x') {
                    "div" => (InvAction::Dividend, SystemCategory::Dividends),
                    "cgshort" => (InvAction::CgDistShort, SystemCategory::CapGainDistShort),
                    _ => (InvAction::CgDistLong, SystemCategory::CapGainDistLong),
                };
                let a = need_amount()?;
                let mut i = match security {
                    Some(s) => {
                        let mut i = item(action);
                        i.security = Some(s);
                        i
                    }
                    None => {
                        let mut i = item(InvAction::MiscIncome);
                        i.counterpart = Some(Counter::System(system));
                        i
                    }
                };
                i.amount = Some(a);
                (vec![i], Some(false))
            }
            "intinc" | "intincx" => {
                let mut i = item(InvAction::Interest);
                i.security = security;
                i.amount = Some(need_amount()?);
                (vec![i], Some(false))
            }
            "reinvdiv" | "reinvlg" | "reinvmd" | "reinvsh" => {
                let action = match act.as_str() {
                    "reinvdiv" => InvAction::ReinvestDividend,
                    "reinvsh" => InvAction::ReinvestCgShort,
                    _ => InvAction::ReinvestCgLong,
                };
                let mut i = item(action);
                i.security = Some(need_security()?);
                i.quantity = Some(need_shares()?);
                i.price = r.price;
                i.amount = amount;
                (vec![i], None)
            }
            "reinvint" => {
                let s = need_security()?;
                let mut income = item(InvAction::Interest);
                income.security = Some(s);
                income.amount = Some(need_amount()?);
                let mut buy = item(InvAction::Buy);
                buy.security = Some(s);
                buy.quantity = Some(need_shares()?);
                buy.price = r.price;
                buy.amount = amount;
                (vec![income, buy], None)
            }
            "shrsin" => {
                let mut i = item(InvAction::SharesAdded);
                i.security = Some(need_security()?);
                let q = need_shares()?;
                i.quantity = Some(q);
                i.price = r.price;
                i.amount = match (amount, r.price) {
                    (Some(a), _) => Some(a),
                    (None, Some(p)) => Some(extended_value(q, p).map_err(|e| e.to_string())?),
                    (None, None) => {
                        warnings.push(format!(
                            "{} has no cost basis; the shares come in at zero cost",
                            r.action
                        ));
                        Some(Money::ZERO)
                    }
                };
                (vec![i], None)
            }
            "shrsout" => {
                let mut i = item(InvAction::SharesRemoved);
                i.security = Some(need_security()?);
                i.quantity = Some(need_shares()?);
                i.price = r.price;
                (vec![i], None)
            }
            "stksplit" => {
                let s = need_security()?;
                let q = need_shares()?.raw();
                let old = 10_000_000;
                if q <= 0 {
                    return Err("a split needs its ratio".into());
                }
                if q == old {
                    warnings.push("a 1:1 split changes nothing; skipped".into());
                    return Ok((Vec::new(), warnings));
                }
                let g = gcd(q, old);
                let mut i = item(InvAction::Split);
                i.security = Some(s);
                i.split = Some(SplitRatio {
                    new: q / g,
                    old: old / g,
                });
                (vec![i], None)
            }
            "rtrncap" | "rtrncapx" => {
                let mut i = item(InvAction::ReturnOfCapital);
                i.security = Some(need_security()?);
                i.amount = Some(need_amount()?);
                (vec![i], Some(false))
            }
            "miscinc" | "miscincx" | "miscexp" | "miscexpx" | "margint" | "margintx" => {
                let income = act.starts_with("miscinc");
                let mut i = item(if income {
                    InvAction::MiscIncome
                } else {
                    InvAction::MiscExpense
                });
                i.security = security;
                i.amount = Some(need_amount()?);
                if !act.ends_with('x') {
                    i.counterpart = cat.map(Counter::Category);
                }
                (vec![i], Some(!income))
            }
            "xin" | "contribx" | "xout" | "withdrwx" | "cash" => {
                let signed = match act.as_str() {
                    "cash" => r
                        .amount
                        .ok_or_else(|| format!("{} needs an amount", r.action))?,
                    "xin" | "contribx" => need_amount()?,
                    _ => need_amount()?.checked_neg().ok_or("amount too large")?,
                };
                if signed.is_zero() {
                    return Ok((Vec::new(), warnings));
                }
                let incoming = !signed.is_negative();
                let counterpart = xfer.map(Counter::Account).or(cat.map(Counter::Category));
                let mut i = match counterpart {
                    Some(c) => {
                        let mut i = item(if incoming {
                            InvAction::CashIn
                        } else {
                            InvAction::CashOut
                        });
                        i.counterpart = Some(c);
                        i
                    }
                    None => item(if incoming {
                        InvAction::MiscIncome
                    } else {
                        InvAction::MiscExpense
                    }),
                };
                i.amount = Some(abs(signed));
                return Ok((vec![i], warnings));
            }
            "reminder" => {
                warnings.push("a reminder, not a transaction; skipped".into());
                return Ok((Vec::new(), warnings));
            }
            _ => return Err(format!("investment action {:?} is not supported", r.action)),
        };

        // The cash an `X` action moves to or from another account.
        let x_action = act.ends_with('x') && !matches!(act.as_str(), "xin" | "xout");
        let mut out = main;
        if let (true, Some(before)) = (x_action, x_before) {
            match (xfer, moved) {
                (Some(acct), Some(m)) if !m.is_zero() => {
                    let mut t = item(if before {
                        InvAction::CashIn
                    } else {
                        InvAction::CashOut
                    });
                    t.amount = Some(m);
                    t.counterpart = Some(Counter::Account(acct));
                    if before {
                        t.order.1 = 0;
                        out.insert(0, t);
                    } else {
                        out.push(t);
                    }
                }
                (Some(_), _) => {}
                (None, _) => warnings.push(format!(
                    "{} names no transfer account; only the {} is imported",
                    r.action,
                    r.action.trim_end_matches(['X', 'x'])
                )),
            }
        }
        Ok((out, warnings))
    }

    fn count_records(&mut self, file: &QifFile) {
        let dates = file
            .bank
            .iter()
            .map(|r| (r.account, r.date))
            .chain(file.invest.iter().map(|r| (r.account, r.date)));
        for (a, d) in dates {
            let acct = &mut self.accounts[a];
            acct.records += 1;
            if let Some(d) = d {
                acct.first = Some(acct.first.map_or(d, |f| f.min(d)));
                acct.last = Some(acct.last.map_or(d, |l| l.max(d)));
            }
        }
    }

    // -----------------------------------------------------------------
    // Accounts
    // -----------------------------------------------------------------

    fn choose_accounts(&mut self) -> Result<()> {
        let book = accounts::list(self.conn)?;
        let wanted: HashMap<String, &AccountChoice> = self
            .options
            .accounts
            .iter()
            .map(|(k, v)| (k.trim().to_lowercase(), v))
            .collect();
        let mut problems = Vec::new();
        let mut existing_used: HashMap<i64, String> = HashMap::new();
        let mut created: HashSet<String> = HashSet::new();
        for a in &mut self.accounts {
            a.default_type = default_type(a.kind, &a.qif_type);
            a.choice = match wanted.get(&a.name.to_lowercase()) {
                Some(c) => (*c).clone(),
                None => match book
                    .iter()
                    .find(|b| b.fields.name.eq_ignore_ascii_case(&a.name))
                {
                    Some(b) => AccountChoice::Existing { id: b.id },
                    None if a.records > 0 => AccountChoice::Create {
                        name: a.name.clone(),
                        account_type: a.default_type,
                    },
                    None => AccountChoice::Skip,
                },
            };
            let mut problem = |m: String| problems.push((a.name.clone(), m));
            match &a.choice {
                AccountChoice::Skip => {}
                AccountChoice::Existing { id } => match book.iter().find(|b| b.id == *id) {
                    None => problem(format!("account {} is not in the book", id.0)),
                    Some(b) => {
                        a.investment_target = b.fields.account_type.is_investment();
                        if b.status == AccountStatus::Closed {
                            problem(format!(
                                "{:?} is closed; reopen it or choose another account",
                                b.fields.name
                            ));
                        }
                        if let Some(other) = existing_used.insert(id.0, a.name.clone()) {
                            problem(format!(
                                "{other:?} also goes to {:?}; each account needs its own",
                                b.fields.name
                            ));
                        }
                    }
                },
                AccountChoice::Create { name, account_type } => {
                    a.investment_target = account_type.is_investment();
                    let n = name.trim();
                    if n.is_empty() {
                        problem("a new account needs a name".into());
                    } else if book.iter().any(|b| b.fields.name.eq_ignore_ascii_case(n)) {
                        problem(format!("the book already has an account named {n:?}"));
                    } else if !created.insert(n.to_lowercase()) {
                        problem(format!("two new accounts would be named {n:?}"));
                    }
                }
            }
            let investment_records = a.kind == Some(AccountKind::Investment);
            if a.records > 0
                && a.kind.is_some()
                && !matches!(a.choice, AccountChoice::Skip)
                && investment_records != a.investment_target
            {
                problem(if investment_records {
                    "has investment transactions; choose an investment account".into()
                } else {
                    "has banking transactions; choose an account that is not an investment account"
                        .into()
                });
            }
        }
        for (account, message) in problems {
            self.errors.push(ImportNote {
                line: None,
                account,
                message,
            });
        }
        Ok(())
    }

    fn skipped(&self, a: usize) -> bool {
        matches!(self.accounts[a].choice, AccountChoice::Skip)
    }

    /// Records of skipped accounts are not imported, nor reported.
    fn drop_skipped(&mut self) {
        let skip: Vec<bool> = (0..self.accounts.len()).map(|a| self.skipped(a)).collect();
        self.items.retain(|i| !skip[i.account()]);
        let errors = std::mem::take(&mut self.record_errors);
        self.errors.extend(
            errors
                .into_iter()
                .filter(|(a, _)| !skip[*a])
                .map(|(_, n)| n),
        );
    }

    /// Transfers to the account itself (Quicken's opening balance) and to
    /// skipped accounts go to Opening Balance.
    fn redirect_transfers(&mut self) {
        let skip: Vec<bool> = (0..self.accounts.len()).map(|a| self.skipped(a)).collect();
        let mut to_skipped = 0;
        for item in &mut self.items {
            match item {
                Item::Bank(b) => {
                    for l in &mut b.lines {
                        if let LineTarget::Account(t) = l.target {
                            if t == b.account || skip[t] {
                                to_skipped += usize::from(t != b.account);
                                l.target = LineTarget::Equity;
                            }
                        }
                    }
                }
                Item::Inv(i) => {
                    if let Some(Counter::Account(t)) = i.counterpart {
                        if t == i.account || skip[t] {
                            to_skipped += usize::from(t != i.account);
                            i.counterpart = Some(Counter::Equity);
                        }
                    }
                }
            }
        }
        if to_skipped > 0 {
            self.warnings.push(ImportNote {
                line: None,
                account: String::new(),
                message: format!(
                    "{to_skipped} transfer(s) to accounts not imported are recorded against \
                     Opening Balance"
                ),
            });
        }
    }

    // -----------------------------------------------------------------
    // Transfers (MIG-070)
    // -----------------------------------------------------------------

    fn legs(&self) -> Vec<Leg> {
        let mut legs = Vec::new();
        for (ix, item) in self.items.iter().enumerate() {
            match item {
                Item::Bank(b) => {
                    for (li, l) in b.lines.iter().enumerate() {
                        if let LineTarget::Account(t) = l.target {
                            legs.push(Leg {
                                item: ix,
                                line: Some(li),
                                from: b.account,
                                to: t,
                                date: b.date,
                                amount: l.amount,
                            });
                        }
                    }
                }
                Item::Inv(i) => {
                    if let (Some(Counter::Account(t)), Some(a)) = (i.counterpart, i.amount) {
                        let amount = if i.action == InvAction::CashOut {
                            a.checked_neg().unwrap_or(a)
                        } else {
                            a
                        };
                        legs.push(Leg {
                            item: ix,
                            line: None,
                            from: i.account,
                            to: t,
                            date: i.date,
                            amount,
                        });
                    }
                }
            }
        }
        legs.sort_by_key(|l| (self.items[l.item].order(), l.line));
        legs
    }

    fn match_transfers(&mut self) {
        let legs = self.legs();
        let mut waiting: HashMap<(usize, usize, Date, i64), Vec<usize>> = HashMap::new();
        let mut matched = vec![false; legs.len()];
        for (x, leg) in legs.iter().enumerate() {
            let want = (leg.to, leg.from, leg.date, -leg.amount.cents());
            let partner = waiting.get_mut(&want).and_then(|q| {
                if q.is_empty() {
                    None
                } else {
                    Some(q.remove(0))
                }
            });
            match partner {
                Some(y) => {
                    matched[x] = true;
                    matched[y] = true;
                    self.pair(&legs[y], leg);
                }
                None => waiting
                    .entry((leg.from, leg.to, leg.date, leg.amount.cents()))
                    .or_default()
                    .push(x),
            }
        }
        self.match_grouped(&legs, &mut matched);
        for (leg, _) in legs.iter().zip(&matched).filter(|(_, m)| !**m) {
            self.unmatched(leg);
        }
    }

    /// A split with several lines to one account may show on the other
    /// side as one entry for their sum: match those too.
    fn match_grouped(&mut self, legs: &[Leg], matched: &mut [bool]) {
        let mut groups: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        for (i, l) in legs.iter().enumerate() {
            if !matched[i] && l.line.is_some() {
                groups.entry((l.item, l.to)).or_default().push(i);
            }
        }
        let mut groups: Vec<Vec<usize>> = groups.into_values().filter(|g| g.len() > 1).collect();
        groups.sort();
        for group in groups {
            let first = &legs[group[0]];
            let sum = group
                .iter()
                .try_fold(Money::ZERO, |acc, &i| acc.checked_add(legs[i].amount));
            let Some(sum) = sum else {
                continue;
            };
            let other = (0..legs.len()).find(|&j| {
                !matched[j]
                    && legs[j].from == first.to
                    && legs[j].to == first.from
                    && legs[j].date == first.date
                    && legs[j].amount.checked_neg() == Some(sum)
            });
            let Some(j) = other else {
                continue;
            };
            matched[j] = true;
            self.transfers_matched += 1;
            let cleared = self.cleared_of(&legs[j]);
            match legs[j].line {
                // A banking entry for the sum: the split keeps its lines.
                Some(_) => {
                    for &i in &group {
                        matched[i] = true;
                        if let (Item::Bank(b), Some(li)) =
                            (&mut self.items[legs[i].item], legs[i].line)
                        {
                            b.lines[li].cleared = cleared;
                        }
                    }
                    self.drop_line(&legs[j]);
                }
                // An investment record for the sum keeps it.
                None => {
                    let bank_cleared = self.cleared_of(first);
                    for &i in &group {
                        matched[i] = true;
                        self.drop_line(&legs[i]);
                    }
                    if let Item::Inv(inv) = &mut self.items[legs[j].item] {
                        inv.counter_cleared = bank_cleared;
                    }
                }
            }
        }
    }

    fn cleared_of(&self, leg: &Leg) -> Cleared {
        match &self.items[leg.item] {
            Item::Bank(b) => b.cleared,
            Item::Inv(i) => i.cleared,
        }
    }

    fn drop_line(&mut self, leg: &Leg) {
        if let (Item::Bank(b), Some(li)) = (&mut self.items[leg.item], leg.line) {
            b.lines[li].dropped = true;
        }
    }

    /// One transfer seen from both sides: keep one, drop the other.
    fn pair(&mut self, first: &Leg, second: &Leg) {
        self.transfers_matched += 1;
        let lines = |me: &Self, l: &Leg| match &me.items[l.item] {
            Item::Bank(b) => b.lines.len(),
            Item::Inv(_) => 0,
        };
        match (first.line, second.line) {
            (Some(_), Some(_)) => {
                let (keep, drop) = if lines(self, second) > lines(self, first) {
                    (second, first)
                } else {
                    (first, second)
                };
                let other = self.cleared_of(drop);
                if let (Item::Bank(b), Some(li)) = (&mut self.items[keep.item], keep.line) {
                    b.lines[li].cleared = other;
                }
                self.drop_line(drop);
            }
            (None, Some(_)) | (Some(_), None) => {
                let (inv, bank) = if first.line.is_none() {
                    (first, second)
                } else {
                    (second, first)
                };
                let other = self.cleared_of(bank);
                if let Item::Inv(i) = &mut self.items[inv.item] {
                    i.counter_cleared = other;
                }
                self.drop_line(bank);
            }
            (None, None) => {
                for leg in [first, second] {
                    if let Item::Inv(i) = &mut self.items[leg.item] {
                        i.counterpart = Some(Counter::Equity);
                    }
                }
                let (a, b) = (
                    self.accounts[first.from].name.clone(),
                    self.accounts[first.to].name.clone(),
                );
                let n = self.note(
                    self.items[first.item].line(),
                    first.from,
                    format!(
                        "cash moved between investment accounts {a:?} and {b:?} on {}; \
                         each side is recorded against Opening Balance",
                        first.date
                    ),
                );
                self.warnings.push(n);
            }
        }
    }

    /// A transfer seen from one side only.
    fn unmatched(&mut self, leg: &Leg) {
        let to = &self.accounts[leg.to];
        let to_name = to.name.clone();
        let to_has_records = to.records > 0;
        match (leg.line, to.investment_target) {
            // Into an investment account: the engine's Cash In or Out.
            (Some(_), true) => {
                let Item::Bank(b) = &self.items[leg.item] else {
                    return;
                };
                let incoming = leg.amount.is_negative();
                let item = InvItem {
                    line: b.line,
                    order: (b.order.0, b.order.1 + 1),
                    account: leg.to,
                    date: b.date,
                    action: if incoming {
                        InvAction::CashIn
                    } else {
                        InvAction::CashOut
                    },
                    security: None,
                    quantity: None,
                    price: None,
                    commission: Money::ZERO,
                    amount: Some(abs(leg.amount)),
                    split: None,
                    counterpart: Some(Counter::Account(leg.from)),
                    memo: memo_with_payee(&b.payee, &b.memo),
                    cleared: Cleared::Unmarked,
                    counter_cleared: b.cleared,
                };
                self.drop_line(leg);
                self.items.push(Item::Inv(item));
            }
            (None, true) => {
                if let Item::Inv(i) = &mut self.items[leg.item] {
                    i.counterpart = Some(Counter::Equity);
                }
                let n = self.note(
                    self.items[leg.item].line(),
                    leg.from,
                    format!(
                        "cash moved to or from investment account {to_name:?} on {}; \
                         recorded against Opening Balance",
                        leg.date
                    ),
                );
                self.warnings.push(n);
            }
            _ if to_has_records => {
                let n = self.note(
                    self.items[leg.item].line(),
                    leg.from,
                    format!(
                        "transfer of {} on {} has no matching entry in {to_name:?}; \
                         imported from this side",
                        leg.amount, leg.date
                    ),
                );
                self.warnings.push(n);
            }
            _ => {}
        }
    }

    /// Take dropped lines out; a record left with nothing goes.
    fn compact(&mut self) {
        self.items.retain_mut(|item| {
            let Item::Bank(b) = item else {
                return true;
            };
            if !b.lines.iter().any(|l| l.dropped) {
                return true;
            }
            for l in b.lines.iter().filter(|l| l.dropped) {
                b.amount = b.amount.checked_sub(l.amount).unwrap_or(b.amount);
            }
            b.lines.retain(|l| !l.dropped);
            !(b.lines.is_empty() && b.amount.is_zero())
        });
        // Two lines to one account become one (an account appears once
        // per transaction).
        for item in &mut self.items {
            if let Item::Bank(b) = item {
                let mut merged: Vec<ItemLine> = Vec::with_capacity(b.lines.len());
                for l in b.lines.drain(..) {
                    let same = merged.iter_mut().find(|m| {
                        matches!(m.target, LineTarget::Account(_)) && m.target == l.target
                    });
                    match same {
                        Some(m) => m.amount = m.amount.checked_add(l.amount).unwrap_or(m.amount),
                        None => merged.push(l),
                    }
                }
                b.lines = merged;
            }
        }
    }

    // -----------------------------------------------------------------
    // Categories, tags, securities, prices, payees
    // -----------------------------------------------------------------

    fn usage(&mut self) {
        for item in &self.items {
            match item {
                Item::Bank(b) => {
                    for t in &b.tags {
                        self.tags[*t].used += 1;
                    }
                    for l in &b.lines {
                        for t in &l.tags {
                            self.tags[*t].used += 1;
                        }
                        if let LineTarget::Category(c) = l.target {
                            let pc = &mut self.categories[c];
                            pc.used += 1;
                            // The category's posting is minus the line.
                            pc.total = pc.total.checked_sub(l.amount).unwrap_or(pc.total);
                        }
                    }
                }
                Item::Inv(i) => {
                    if let Some(s) = i.security {
                        self.securities[s].used += 1;
                    }
                    if let Some(Counter::Category(c)) = i.counterpart {
                        let a = i.amount.unwrap_or(Money::ZERO);
                        let posting = if i.action.cash_direction() > 0 {
                            a.checked_neg().unwrap_or(a)
                        } else {
                            a
                        };
                        let pc = &mut self.categories[c];
                        pc.used += 1;
                        pc.total = pc.total.checked_add(posting).unwrap_or(pc.total);
                    }
                }
            }
        }
    }

    fn choose_categories(&mut self) -> Result<()> {
        // Book categories by path, lower-cased.
        let list = categories::list(self.conn)?;
        let mut paths: HashMap<CategoryId, String> = HashMap::new();
        let mut by_path: HashMap<String, (CategoryId, CategoryKind)> = HashMap::new();
        for c in &list {
            let path = match c.fields.parent.and_then(|p| paths.get(&p)) {
                Some(parent) => format!("{parent}:{}", c.fields.name),
                None => c.fields.name.clone(),
            };
            by_path.insert(path.to_lowercase(), (c.id, c.fields.kind));
            paths.insert(c.id, path);
        }
        let wanted: HashMap<String, &CategoryChoice> = self
            .options
            .categories
            .iter()
            .map(|(k, v)| (k.trim().to_lowercase(), v))
            .collect();
        let keep: HashSet<String> = self
            .options
            .keep_categories
            .iter()
            .map(|k| k.trim().to_lowercase())
            .collect();
        let mut problems = Vec::new();
        for c in &mut self.categories {
            let kind = match c.income {
                Some(true) => CategoryKind::Income,
                Some(false) => CategoryKind::Expense,
                None if c.total.is_negative() => CategoryKind::Income,
                None => CategoryKind::Expense,
            };
            let key = c.name.to_lowercase();
            c.imported = c.used > 0 || keep.contains(&key);
            let path = if c.name.is_empty() {
                UNCATEGORIZED.to_string()
            } else {
                c.name.clone()
            };
            c.choice = match wanted.get(&key) {
                Some(w) => (*w).clone(),
                None => match by_path.get(&path.to_lowercase()) {
                    Some((id, _)) => CategoryChoice::Existing { id: *id },
                    None => CategoryChoice::Create {
                        path,
                        category_kind: kind,
                    },
                },
            };
            if !c.imported {
                continue;
            }
            let shown = if c.name.is_empty() {
                "(no category)".to_string()
            } else {
                c.name.clone()
            };
            match &c.choice {
                CategoryChoice::Existing { id } => {
                    if !paths.contains_key(id) {
                        problems.push((shown, format!("category {} is not in the book", id.0)));
                    }
                }
                CategoryChoice::Create {
                    path,
                    category_kind: kind,
                } => {
                    if *kind == CategoryKind::Equity {
                        problems.push((shown, "equity categories are built in".into()));
                        continue;
                    }
                    let parts: Vec<&str> = path.split(':').map(str::trim).collect();
                    if parts.iter().any(|p| p.is_empty()) {
                        problems.push((shown, format!("{path:?} is not a category path")));
                        continue;
                    }
                    // The levels already in the book must be of this kind.
                    let mut prefix = String::new();
                    for p in &parts {
                        if !prefix.is_empty() {
                            prefix.push(':');
                        }
                        prefix.push_str(&p.to_lowercase());
                        if let Some((_, k)) = by_path.get(&prefix) {
                            if k != kind {
                                problems.push((
                                    shown.clone(),
                                    format!("{path:?} would put a {kind} category under a {k} one"),
                                ));
                                break;
                            }
                        }
                    }
                }
            }
        }
        for (account, message) in problems {
            self.errors.push(ImportNote {
                line: None,
                account: format!("Category {account}"),
                message,
            });
        }
        Ok(())
    }

    fn choose_securities(&mut self) -> Result<()> {
        let wanted: HashMap<String, &SecurityChoice> = self
            .options
            .securities
            .iter()
            .map(|(k, v)| (k.trim().to_lowercase(), v))
            .collect();
        let keep: HashSet<String> = self
            .options
            .keep_securities
            .iter()
            .map(|k| k.trim().to_lowercase())
            .collect();
        let mut problems = Vec::new();
        let mut tickers: HashSet<String> = HashSet::new();
        for s in &mut self.securities {
            let key = s.name.to_lowercase();
            s.imported = s.used > 0 || keep.contains(&key);
            s.choice = match wanted.get(&key) {
                Some(w) => (*w).clone(),
                None => {
                    let by_ticker = match &s.symbol {
                        Some(t) => securities::find_by_ticker(self.conn, t)?,
                        None => None,
                    };
                    let found = match by_ticker {
                        Some(f) => Some(f),
                        None => securities::find_by_label(self.conn, &s.name)?,
                    };
                    match found {
                        Some(f) => SecurityChoice::Existing { id: f.id },
                        None => SecurityChoice::Create {
                            name: s.name.clone(),
                            ticker: s.symbol.clone(),
                            security_type: security_type(&s.qif_type),
                        },
                    }
                }
            };
            if !s.imported {
                continue;
            }
            match &s.choice {
                SecurityChoice::Existing { id } => {
                    if securities::find(self.conn, *id)?.is_none() {
                        problems.push((
                            s.name.clone(),
                            format!("security {} is not in the book", id.0),
                        ));
                    }
                }
                SecurityChoice::Create { name, ticker, .. } => {
                    if name.trim().is_empty() {
                        problems.push((s.name.clone(), "a new security needs a name".into()));
                    }
                    if let Some(t) = ticker.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                        if securities::find_by_ticker(self.conn, t)?.is_some() {
                            problems.push((
                                s.name.clone(),
                                format!("the book already has a security with ticker {t}"),
                            ));
                        } else if !tickers.insert(t.to_uppercase()) {
                            problems.push((
                                s.name.clone(),
                                format!("two new securities would have ticker {t}"),
                            ));
                        }
                    }
                }
            }
        }
        for (name, message) in problems {
            self.errors.push(ImportNote {
                line: None,
                account: format!("Security {name}"),
                message,
            });
        }
        Ok(())
    }

    fn choose_tags(&mut self) -> Result<()> {
        let book = tags::list(self.conn)?;
        let keep: HashSet<String> = self
            .options
            .keep_tags
            .iter()
            .map(|k| k.trim().to_lowercase())
            .collect();
        for t in &mut self.tags {
            t.existing = book
                .iter()
                .find(|b| b.fields.name.eq_ignore_ascii_case(&t.name))
                .map(|b| b.id);
            t.imported = t.used > 0 || keep.contains(&t.name.to_lowercase());
        }
        Ok(())
    }

    /// Prices of the securities kept (MIG-140), one per security and date
    /// (the file's last wins).
    fn prices(&mut self, file: &QifFile) -> Vec<PlanPrice> {
        let by_symbol: HashMap<String, usize> = self
            .securities
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.symbol.as_ref().map(|t| (t.to_lowercase(), i)))
            .collect();
        let mut out: Vec<PlanPrice> = Vec::new();
        let mut at: HashMap<(usize, Date), usize> = HashMap::new();
        for p in &file.prices {
            let (Some(price), Some(date)) = (p.price, p.date) else {
                continue;
            };
            let Some(&s) = by_symbol.get(&p.symbol.to_lowercase()) else {
                continue;
            };
            if price.raw() <= 0 {
                continue;
            }
            self.securities[s].prices += 1;
            if !self.options.prices || !self.securities[s].imported {
                continue;
            }
            match at.get(&(s, date)) {
                Some(&i) => out[i].price = price,
                None => {
                    at.insert((s, date), out.len());
                    out.push(PlanPrice {
                        security: s,
                        date,
                        price,
                    });
                }
            }
        }
        out
    }

    fn new_payees(&self) -> Result<i64> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut new = 0;
        for item in &self.items {
            if let Item::Bank(b) = item {
                if !b.payee.is_empty()
                    && seen.insert(b.payee.to_lowercase())
                    && payees::find_by_name(self.conn, &b.payee)?.is_none()
                {
                    new += 1;
                }
            }
        }
        Ok(new)
    }

    /// What the import adds to each account's balance (cash for an
    /// investment account), for the preview (MIG-050).
    fn totals(&mut self) {
        let add = |accts: &mut Vec<PlanAccount>, a: usize, m: Money| {
            let t = &mut accts[a].total;
            *t = t.checked_add(m).unwrap_or(*t);
        };
        for item in &self.items {
            match item {
                Item::Bank(b) => {
                    add(&mut self.accounts, b.account, b.amount);
                    for l in &b.lines {
                        if let LineTarget::Account(t) = l.target {
                            add(
                                &mut self.accounts,
                                t,
                                l.amount.checked_neg().unwrap_or(l.amount),
                            );
                        }
                    }
                }
                Item::Inv(i) => {
                    let a = match (i.amount, i.quantity, i.price) {
                        (Some(a), _, _) => a,
                        (None, Some(q), Some(p)) => {
                            crate::invest::trade_amount(i.action, q, p, i.commission)
                                .unwrap_or(Money::ZERO)
                        }
                        _ => Money::ZERO,
                    };
                    let cash = match i.action.cash_direction() {
                        d if d > 0 => a,
                        d if d < 0 => a.checked_neg().unwrap_or(a),
                        _ => Money::ZERO,
                    };
                    add(&mut self.accounts, i.account, cash);
                    if let Some(Counter::Account(t)) = i.counterpart {
                        add(&mut self.accounts, t, cash.checked_neg().unwrap_or(cash));
                    }
                }
            }
        }
    }
}

impl Plan {
    pub fn preview(&self, file: &QifFile, file_name: &str) -> ImportPreview {
        let dates = self.items.iter().map(Item::date);
        ImportPreview {
            file_name: file_name.to_string(),
            date_order: file.date_order,
            date_ambiguous: file.date_ambiguous,
            first_date: dates.clone().min(),
            last_date: dates.max(),
            accounts: self
                .accounts
                .iter()
                .map(|a| AccountPreview {
                    name: a.name.clone(),
                    qif_type: a.qif_type.clone(),
                    investment: a.kind == Some(AccountKind::Investment),
                    defined: a.defined,
                    records: a.records,
                    first_date: a.first,
                    last_date: a.last,
                    total: a.total,
                    choice: a.choice.clone(),
                    default_type: a.default_type,
                })
                .collect(),
            categories: self
                .categories
                .iter()
                .map(|c| CategoryPreview {
                    name: c.name.clone(),
                    listed: c.listed,
                    kind: match &c.choice {
                        CategoryChoice::Create { category_kind, .. } => *category_kind,
                        CategoryChoice::Existing { .. } => match c.income {
                            Some(true) => CategoryKind::Income,
                            Some(false) => CategoryKind::Expense,
                            None if c.total.is_negative() => CategoryKind::Income,
                            None => CategoryKind::Expense,
                        },
                    },
                    used: c.used,
                    total: c.total,
                    choice: c.choice.clone(),
                    imported: c.imported,
                })
                .collect(),
            tags: self
                .tags
                .iter()
                .map(|t| TagPreview {
                    name: t.name.clone(),
                    used: t.used,
                    existing: t.existing.is_some(),
                    imported: t.imported,
                })
                .collect(),
            securities: self
                .securities
                .iter()
                .map(|s| SecurityPreview {
                    name: s.name.clone(),
                    symbol: s.symbol.clone(),
                    qif_type: s.qif_type.clone(),
                    used: s.used,
                    prices: s.prices,
                    choice: s.choice.clone(),
                    imported: s.imported,
                })
                .collect(),
            transactions: count(self.items.len()),
            transfers_matched: self.transfers_matched,
            new_payees: self.new_payees,
            prices: count(self.prices.len()),
            memorized_skipped: count(file.memorized),
            warnings: self.warnings.clone(),
            errors: self.errors.clone(),
            imported_before: None,
        }
    }
}
