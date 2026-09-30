//! QIF reader (MIG-010, MIG-160): text in, records out. No database.
//!
//! Handles what Quicken 2013 writes in a whole-file or per-account
//! export: the account list (`!Account`, with or without AutoSwitch),
//! categories (`!Type:Cat`), tags (`!Type:Tag`, older `!Type:Class`),
//! securities, prices, and banking (`Bank`, `Cash`, `CCard`, `Oth A`,
//! `Oth L`) and investment (`Invst`) transactions. Memorized transactions
//! are counted and skipped; other sections are skipped with a note.
//!
//! Pitfalls (MIG-160): two-digit years (`1/5/98` is 1998, `1/5'26` is
//! 2026), padded fields (`1/ 5'26`), day-first dates (decided from the
//! whole file, or given), thousands commas, split lines (`S`/`E`/`$`),
//! `[Account]` transfers, `Category/Tag`, fractional prices (`12 1/2`),
//! Windows-1252 text. Nothing here fails: problems stay on the record
//! they belong to, or become a file note.

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};

use crate::date::Date;
use crate::ledger::Cleared;
use crate::money::{Money, Price, Quantity};
use crate::text_enum::text_enum;

text_enum! {
    /// Order of the day and month in the file's dates.
    pub enum DateOrder {
        /// Month first (US): `1/5'26` is 5 January 2026.
        Mdy = "mdy",
        /// Day first: `1/5'26` is 1 May 2026.
        Dmy = "dmy",
    }
}

/// What a QIF account holds, from its `T` line or section header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountKind {
    Bank,
    Cash,
    CreditCard,
    Investment,
    OtherAsset,
    OtherLiability,
}

impl AccountKind {
    /// From an account's `T` value or a `!Type:` header.
    pub fn from_type(t: &str) -> Option<AccountKind> {
        Some(match t.trim().to_ascii_lowercase().as_str() {
            "bank" => AccountKind::Bank,
            "cash" => AccountKind::Cash,
            "ccard" => AccountKind::CreditCard,
            "invst" | "port" | "mutual" | "401(k)/403(b)" | "401(k)" => AccountKind::Investment,
            "oth a" => AccountKind::OtherAsset,
            "oth l" => AccountKind::OtherLiability,
            _ => return None,
        })
    }
}

/// An account named in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QifAccount {
    pub name: String,
    /// The `T` value as written (`Bank`, `CCard`, `401(k)/403(b)`, …).
    pub qif_type: String,
    pub kind: Option<AccountKind>,
    pub description: String,
    pub credit_limit: Option<Money>,
    /// Named by an `!Account` record (not only by a transfer).
    pub defined: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QifCategory {
    pub name: String,
    pub description: String,
    /// `I` (income) or `E` (expense); `None` when neither is given.
    pub income: Option<bool>,
    pub tax_related: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QifTag {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QifSecurity {
    pub name: String,
    pub symbol: Option<String>,
    pub qif_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QifPrice {
    pub line: usize,
    pub symbol: String,
    pub price: Option<Price>,
    pub date: Option<Date>,
    date_text: String,
}

/// One split line of a banking record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QifSplit {
    /// `S`: a category, `[Account]`, either with `/Tag`; may be empty.
    pub category: String,
    pub memo: String,
    pub amount: Option<Money>,
}

/// A banking, cash, credit card, or other asset/liability transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankRecord {
    /// First line of the record (1-based).
    pub line: usize,
    /// Index into [`QifFile::accounts`].
    pub account: usize,
    pub date: Option<Date>,
    /// `T` (or `U`), signed as the account sees it.
    pub amount: Option<Money>,
    pub cleared: Cleared,
    /// `N`: check number or a word like `DEP`.
    pub num: String,
    pub payee: String,
    pub memo: String,
    /// `L`: a category, `[Account]`, either with `/Tag`; may be empty.
    pub category: String,
    pub splits: Vec<QifSplit>,
    /// Quicken exports a void as a zero amount with `**VOID**` before
    /// the payee (removed here).
    pub void: bool,
    pub problems: Vec<String>,
    date_text: String,
}

/// An investment transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvRecord {
    pub line: usize,
    pub account: usize,
    pub date: Option<Date>,
    /// `N`: `Buy`, `SellX`, `ReinvDiv`, …
    pub action: String,
    /// `Y`: the security's name.
    pub security: String,
    /// `I`.
    pub price: Option<Price>,
    /// `Q`: shares; for `StkSplit`, new shares per 10 old.
    pub quantity: Option<Quantity>,
    /// `T` (or `U`).
    pub amount: Option<Money>,
    pub cleared: Cleared,
    /// `P`: text only (investment transactions have no payee).
    pub payee: String,
    pub memo: String,
    /// `O`.
    pub commission: Option<Money>,
    /// `L`: `[Account]` for the `X` actions, a category for misc ones.
    pub category: String,
    /// `$`: the amount transferred.
    pub transfer_amount: Option<Money>,
    pub problems: Vec<String>,
    date_text: String,
}

/// A note about the file itself, not one record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileNote {
    pub line: usize,
    pub message: String,
}

/// Everything read from one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QifFile {
    pub accounts: Vec<QifAccount>,
    pub categories: Vec<QifCategory>,
    pub tags: Vec<QifTag>,
    pub securities: Vec<QifSecurity>,
    pub prices: Vec<QifPrice>,
    pub bank: Vec<BankRecord>,
    pub invest: Vec<InvRecord>,
    pub notes: Vec<FileNote>,
    /// Memorized transactions skipped (never imported, MIG-160).
    pub memorized: usize,
    pub date_order: DateOrder,
    /// No date showed which comes first; month first was assumed.
    pub date_ambiguous: bool,
}

impl QifFile {
    /// Index of the account named `name` (ignoring case).
    pub fn account_index(&self, name: &str) -> Option<usize> {
        let name = name.trim();
        self.accounts
            .iter()
            .position(|a| a.name.eq_ignore_ascii_case(name))
    }
}

/// Text of a file as Quicken wrote it: UTF-8 if it is valid UTF-8,
/// otherwise Windows-1252 (Quicken 2013 on Windows). A byte-order mark is
/// dropped.
pub fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| cp1252(b)).collect(),
    }
}

/// Windows-1252 byte to char. 0x80–0x9F differ from Latin-1; the five
/// undefined bytes keep their Latin-1 (control) meaning.
fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž',
        '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}',
        'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => HIGH[usize::from(b - 0x80)],
        _ => char::from(b),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    AccountList,
    Category,
    Tag,
    Security,
    Prices,
    Memorized,
    Bank,
    Invest,
    Other,
}

/// Read `text`. Records outside any `!Account` go to an account named
/// `default_account` (a per-account export names none). `order` decides
/// day and month; `None` works it out from the file's dates.
pub fn parse(text: &str, default_account: &str, order: Option<DateOrder>) -> QifFile {
    let mut p = Parser {
        file: QifFile {
            accounts: Vec::new(),
            categories: Vec::new(),
            tags: Vec::new(),
            securities: Vec::new(),
            prices: Vec::new(),
            bank: Vec::new(),
            invest: Vec::new(),
            notes: Vec::new(),
            memorized: 0,
            date_order: DateOrder::Mdy,
            date_ambiguous: false,
        },
        section: Section::None,
        current: None,
        default_account: default_account.trim().to_string(),
        fields: Vec::new(),
        skipped_sections: Vec::new(),
    };
    for (i, raw) in text.lines().enumerate() {
        p.line(i + 1, raw.trim_end_matches('\r'));
    }
    p.end_record();
    p.resolve_dates(order);
    p.file
}

struct Parser {
    file: QifFile,
    section: Section,
    /// The account the next transactions belong to.
    current: Option<usize>,
    default_account: String,
    /// Fields of the record being read: (line, code, value).
    fields: Vec<(usize, char, String)>,
    skipped_sections: Vec<String>,
}

impl Parser {
    fn note(&mut self, line: usize, message: impl Into<String>) {
        self.file.notes.push(FileNote {
            line,
            message: message.into(),
        });
    }

    fn line(&mut self, n: usize, l: &str) {
        if l.trim().is_empty() {
            return;
        }
        if l.starts_with('!') {
            self.end_record();
            self.header(n, l.trim());
            return;
        }
        if l.starts_with('^') {
            self.end_record();
            return;
        }
        if self.section == Section::Prices {
            self.price_line(n, l.trim());
            return;
        }
        let mut chars = l.chars();
        let Some(code) = chars.next() else {
            return;
        };
        self.fields
            .push((n, code, chars.as_str().trim().to_string()));
    }

    fn header(&mut self, n: usize, h: &str) {
        let lower = h.to_ascii_lowercase();
        if lower.starts_with("!option") || lower.starts_with("!clear") {
            return;
        }
        if lower == "!account" {
            self.section = Section::AccountList;
            return;
        }
        let Some(kind) = lower.strip_prefix("!type:") else {
            self.note(n, format!("unknown header {h:?}; its records are skipped"));
            self.section = Section::Other;
            return;
        };
        let kind = kind.trim();
        self.section = match kind {
            "cat" => Section::Category,
            "tag" | "class" => Section::Tag,
            "security" => Section::Security,
            "prices" => Section::Prices,
            "memorized" => Section::Memorized,
            "invst" => {
                self.transaction_account(AccountKind::Investment, "Invst");
                Section::Invest
            }
            _ => match AccountKind::from_type(kind) {
                Some(k) => {
                    let label = &h["!Type:".len()..];
                    self.transaction_account(k, label.trim());
                    Section::Bank
                }
                None => {
                    if !self.skipped_sections.iter().any(|s| s == kind) {
                        self.skipped_sections.push(kind.to_string());
                        self.note(n, format!("section {h:?} is not imported"));
                    }
                    Section::Other
                }
            },
        };
    }

    /// The account a transaction section belongs to: the last `!Account`,
    /// else the default one. Gives it a kind if it has none.
    fn transaction_account(&mut self, kind: AccountKind, label: &str) {
        let ix = match self.current {
            Some(ix) => ix,
            None => {
                let name = if self.default_account.is_empty() {
                    "Imported account".to_string()
                } else {
                    self.default_account.clone()
                };
                let ix = self.account(&name, false);
                self.current = Some(ix);
                ix
            }
        };
        let a = &mut self.file.accounts[ix];
        if a.kind.is_none() {
            a.kind = Some(kind);
            if a.qif_type.is_empty() {
                a.qif_type = label.to_string();
            }
        }
    }

    /// Index of the account named `name`, added if new.
    fn account(&mut self, name: &str, defined: bool) -> usize {
        match self.file.account_index(name) {
            Some(ix) => {
                self.file.accounts[ix].defined |= defined;
                ix
            }
            None => {
                self.file.accounts.push(QifAccount {
                    name: name.trim().to_string(),
                    qif_type: String::new(),
                    kind: None,
                    description: String::new(),
                    credit_limit: None,
                    defined,
                });
                self.file.accounts.len() - 1
            }
        }
    }

    fn end_record(&mut self) {
        if self.fields.is_empty() {
            return;
        }
        let fields = std::mem::take(&mut self.fields);
        let line = fields[0].0;
        match self.section {
            Section::AccountList => self.account_record(line, &fields),
            Section::Category => self.category_record(&fields),
            Section::Tag => self.tag_record(&fields),
            Section::Security => self.security_record(&fields),
            Section::Memorized => self.file.memorized += 1,
            Section::Bank => self.bank_record(line, &fields),
            Section::Invest => self.invest_record(line, &fields),
            Section::None => self.note(line, "data before any header; skipped"),
            Section::Prices | Section::Other => {}
        }
    }

    fn account_record(&mut self, line: usize, fields: &[(usize, char, String)]) {
        let name = field(fields, 'N');
        if name.is_empty() {
            self.note(line, "an account without a name; skipped");
            return;
        }
        let ix = self.account(name, true);
        self.current = Some(ix);
        let t = field(fields, 'T');
        let a = &mut self.file.accounts[ix];
        if !t.is_empty() {
            a.qif_type = t.to_string();
            a.kind = AccountKind::from_type(t).or(a.kind);
        }
        let d = field(fields, 'D');
        if !d.is_empty() {
            a.description = d.to_string();
        }
        let limit = field(fields, 'L');
        if !limit.is_empty() {
            a.credit_limit = money(limit).ok();
        }
    }

    fn category_record(&mut self, fields: &[(usize, char, String)]) {
        let name = field(fields, 'N');
        if name.is_empty() {
            return;
        }
        let has = |c: char| fields.iter().any(|(_, k, _)| *k == c);
        self.file.categories.push(QifCategory {
            name: name.to_string(),
            description: field(fields, 'D').to_string(),
            income: if has('I') {
                Some(true)
            } else if has('E') {
                Some(false)
            } else {
                None
            },
            tax_related: has('T'),
        });
    }

    fn tag_record(&mut self, fields: &[(usize, char, String)]) {
        let name = field(fields, 'N');
        if !name.is_empty() {
            self.file.tags.push(QifTag {
                name: name.to_string(),
                description: field(fields, 'D').to_string(),
            });
        }
    }

    fn security_record(&mut self, fields: &[(usize, char, String)]) {
        let name = field(fields, 'N');
        if name.is_empty() {
            return;
        }
        let symbol = field(fields, 'S');
        self.file.securities.push(QifSecurity {
            name: name.to_string(),
            symbol: (!symbol.is_empty()).then(|| symbol.to_string()),
            qif_type: field(fields, 'T').to_string(),
        });
    }

    fn price_line(&mut self, n: usize, l: &str) {
        let parts = split_quoted(l);
        let symbol = parts.first().map(|s| s.trim()).unwrap_or("");
        if symbol.is_empty() {
            return;
        }
        let price = parts
            .get(1)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .and_then(|s| scaled6(s).ok())
            .map(Price::from_raw);
        self.file.prices.push(QifPrice {
            line: n,
            symbol: symbol.to_string(),
            price,
            date: None,
            date_text: parts
                .get(2)
                .map(|s| s.trim().to_string())
                .unwrap_or_default(),
        });
    }

    fn bank_record(&mut self, line: usize, fields: &[(usize, char, String)]) {
        let Some(account) = self.current else {
            return;
        };
        let mut r = BankRecord {
            line,
            account,
            date: None,
            amount: None,
            cleared: Cleared::Unmarked,
            num: String::new(),
            payee: String::new(),
            memo: String::new(),
            category: String::new(),
            splits: Vec::new(),
            void: false,
            problems: Vec::new(),
            date_text: String::new(),
        };
        let mut t: Option<&str> = None;
        let mut u: Option<&str> = None;
        for (_, code, value) in fields {
            let v = value.as_str();
            match code {
                'D' => r.date_text = v.to_string(),
                'T' => t = Some(v),
                'U' => u = Some(v),
                'C' => r.cleared = cleared(v),
                'N' => r.num = v.to_string(),
                'P' => r.payee = v.to_string(),
                'M' => r.memo = v.to_string(),
                'L' => r.category = v.to_string(),
                'S' => r.splits.push(QifSplit {
                    category: v.to_string(),
                    memo: String::new(),
                    amount: None,
                }),
                'E' => last_split(&mut r.splits).memo = v.to_string(),
                '$' => match money(v) {
                    Ok(m) => last_split(&mut r.splits).amount = Some(m),
                    Err(e) => r.problems.push(format!("split amount {v:?}: {e}")),
                },
                _ => {}
            }
        }
        if let Some(v) = t.or(u).filter(|v| !v.is_empty()) {
            match money(v) {
                Ok(m) => r.amount = Some(m),
                Err(e) => r.problems.push(format!("amount {v:?}: {e}")),
            }
        }
        if let Some(rest) = r.payee.strip_prefix("**VOID**") {
            r.payee = rest.trim().to_string();
            r.void = true;
        }
        self.file.bank.push(r);
    }

    fn invest_record(&mut self, line: usize, fields: &[(usize, char, String)]) {
        let Some(account) = self.current else {
            return;
        };
        let mut r = InvRecord {
            line,
            account,
            date: None,
            action: String::new(),
            security: String::new(),
            price: None,
            quantity: None,
            amount: None,
            cleared: Cleared::Unmarked,
            payee: String::new(),
            memo: String::new(),
            commission: None,
            category: String::new(),
            transfer_amount: None,
            problems: Vec::new(),
            date_text: String::new(),
        };
        let mut t: Option<&str> = None;
        let mut u: Option<&str> = None;
        for (_, code, value) in fields {
            let v = value.as_str();
            let mut num = |what: &str, parse: fn(&str) -> Result<i64, String>| -> Option<i64> {
                if v.is_empty() {
                    return None;
                }
                match parse(v) {
                    Ok(n) => Some(n),
                    Err(e) => {
                        r.problems.push(format!("{what} {v:?}: {e}"));
                        None
                    }
                }
            };
            match code {
                'D' => r.date_text = v.to_string(),
                'N' => r.action = v.to_string(),
                'Y' => r.security = v.to_string(),
                'I' => r.price = num("price", scaled6).map(Price::from_raw),
                'Q' => r.quantity = num("shares", scaled6).map(Quantity::from_raw),
                'O' => r.commission = num("commission", cents).map(Money::from_cents),
                '$' => r.transfer_amount = num("transfer amount", cents).map(Money::from_cents),
                'T' => t = Some(v),
                'U' => u = Some(v),
                'C' => r.cleared = cleared(v),
                'P' => r.payee = v.to_string(),
                'M' => r.memo = v.to_string(),
                'L' => r.category = v.to_string(),
                _ => {}
            }
        }
        if let Some(v) = t.or(u).filter(|v| !v.is_empty()) {
            match money(v) {
                Ok(m) => r.amount = Some(m),
                Err(e) => r.problems.push(format!("amount {v:?}: {e}")),
            }
        }
        self.file.invest.push(r);
    }

    /// Settle day/month order from every date in the file, then read them.
    fn resolve_dates(&mut self, order: Option<DateOrder>) {
        let f = &mut self.file;
        let raws: Vec<RawDate> = f
            .bank
            .iter()
            .map(|r| r.date_text.as_str())
            .chain(f.invest.iter().map(|r| r.date_text.as_str()))
            .chain(f.prices.iter().map(|p| p.date_text.as_str()))
            .filter_map(raw_date)
            .collect();
        let day_first = raws.iter().any(|d| !d.iso && d.a > 12);
        let month_first = raws.iter().any(|d| !d.iso && d.b > 12);
        f.date_order = match order {
            Some(o) => o,
            None if day_first && !month_first => DateOrder::Dmy,
            None => DateOrder::Mdy,
        };
        f.date_ambiguous = order.is_none()
            && !day_first
            && !month_first
            && raws.iter().any(|d| !d.iso && d.a != d.b);
        let o = f.date_order;
        for r in &mut f.bank {
            match read_date(&r.date_text, o) {
                Ok(d) => r.date = Some(d),
                Err(e) => r.problems.push(e),
            }
        }
        for r in &mut f.invest {
            match read_date(&r.date_text, o) {
                Ok(d) => r.date = Some(d),
                Err(e) => r.problems.push(e),
            }
        }
        for p in &mut f.prices {
            p.date = read_date(&p.date_text, o).ok();
        }
    }
}

fn field(fields: &[(usize, char, String)], code: char) -> &str {
    fields
        .iter()
        .find(|(_, c, _)| *c == code)
        .map_or("", |(_, _, v)| v.as_str())
}

fn last_split(splits: &mut Vec<QifSplit>) -> &mut QifSplit {
    if splits.is_empty() {
        splits.push(QifSplit {
            category: String::new(),
            memo: String::new(),
            amount: None,
        });
    }
    let last = splits.len() - 1;
    &mut splits[last]
}

/// `C` field: `*` or `c` cleared, `X` or `R` reconciled (MIG-090).
fn cleared(v: &str) -> Cleared {
    match v.trim() {
        "*" | "c" | "C" => Cleared::Cleared,
        "X" | "x" | "R" | "r" => Cleared::Reconciled,
        _ => Cleared::Unmarked,
    }
}

/// Split a price line on commas outside double quotes; quotes removed.
fn split_quoted(l: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in l.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// A number as Quicken writes it: thousands commas, maybe spaces, maybe a
/// fraction (`12 1/2`, `3/8`).
fn decimal(s: &str) -> Result<Decimal, String> {
    let s = s.trim();
    if let Some((whole, frac)) = s.rsplit_once(' ').filter(|(_, f)| f.contains('/')) {
        let w = decimal(whole)?;
        let f = fraction(frac)?;
        return Ok(if w.is_sign_negative() { w - f } else { w + f });
    }
    if s.contains('/') {
        return fraction(s);
    }
    let cleaned: String = s
        .chars()
        .filter(|c| !matches!(c, ',' | '$') && !c.is_whitespace())
        .collect();
    if cleaned.is_empty() {
        return Err("no number".into());
    }
    cleaned
        .parse::<Decimal>()
        .map_err(|_| "not a number".to_string())
}

fn fraction(s: &str) -> Result<Decimal, String> {
    let (n, d) = s.split_once('/').ok_or("not a fraction")?;
    let n: Decimal = n.trim().parse().map_err(|_| "not a fraction")?;
    let d: Decimal = d.trim().parse().map_err(|_| "not a fraction")?;
    if d.is_zero() {
        return Err("a fraction over zero".into());
    }
    n.checked_div(d).ok_or_else(|| "not a fraction".into())
}

/// Money, rounded half-even to cents.
pub(crate) fn money(s: &str) -> Result<Money, String> {
    Money::from_decimal(decimal(s)?).map_err(|e| e.to_string())
}

fn cents(s: &str) -> Result<i64, String> {
    money(s).map(Money::cents)
}

/// Shares or a price, rounded half-even to 6 places, as the raw integer.
fn scaled6(s: &str) -> Result<i64, String> {
    decimal(s)?
        .checked_mul(Decimal::from(1_000_000))
        .map(|d| d.round_dp_with_strategy(0, RoundingStrategy::MidpointNearestEven))
        .and_then(|d| d.to_i64())
        .ok_or_else(|| "too large".into())
}

/// A date's three numbers before day and month are decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RawDate {
    a: u32,
    b: u32,
    year: i32,
    /// Year first (`2026-01-05`): no question of order.
    iso: bool,
}

fn raw_date(s: &str) -> Option<RawDate> {
    let mut nums: Vec<(String, char)> = Vec::new();
    let mut cur = String::new();
    let mut last_sep = ' ';
    for c in s.trim().chars() {
        if c.is_ascii_digit() {
            cur.push(c);
        } else if matches!(c, '/' | '\'' | '-' | '.') {
            if cur.is_empty() {
                return None;
            }
            nums.push((std::mem::take(&mut cur), last_sep));
            last_sep = c;
        } else if !c.is_whitespace() {
            return None;
        }
    }
    if cur.is_empty() {
        return None;
    }
    nums.push((cur, last_sep));
    let [(n1, _), (n2, _), (n3, sep3)] = nums.as_slice() else {
        return None;
    };
    let num = |t: &str| t.parse::<u32>().ok();
    if n1.len() == 4 {
        return Some(RawDate {
            a: num(n2)?,
            b: num(n3)?,
            year: i32::try_from(num(n1)?).ok()?,
            iso: true,
        });
    }
    let y = num(n3)?;
    let year = match n3.len() {
        4 => i32::try_from(y).ok()?,
        1 | 2 if *sep3 == '\'' => 2000 + i32::try_from(y).ok()?,
        1 | 2 => 1900 + i32::try_from(y).ok()?,
        _ => return None,
    };
    Some(RawDate {
        a: num(n1)?,
        b: num(n2)?,
        year,
        iso: false,
    })
}

/// Read a QIF date (MIG-160) with the given day/month order.
pub fn read_date(s: &str, order: DateOrder) -> Result<Date, String> {
    if s.trim().is_empty() {
        return Err("no date".into());
    }
    let bad = || format!("date {s:?} is not a date");
    let r = raw_date(s).ok_or_else(bad)?;
    let (month, day) = if r.iso {
        (r.a, r.b)
    } else {
        match order {
            DateOrder::Mdy => (r.a, r.b),
            DateOrder::Dmy => (r.b, r.a),
        }
    };
    Date::from_ymd(r.year, month, day).map_err(|_| bad())
}

/// A category or transfer field (`L`, `S`) taken apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    None,
    Category(String),
    /// `[Account Name]`.
    Transfer(String),
}

/// Split `Category/Tag`, `[Account]/Tag`, or `/Tag`. Several tags may be
/// separated by `:`.
pub fn split_target(s: &str) -> (Target, Vec<String>) {
    let s = s.trim();
    let (head, tags) = if let Some(rest) = s.strip_prefix('[') {
        match rest.find(']') {
            Some(end) => {
                let name = rest[..end].trim();
                let after = rest[end + 1..].trim();
                let tags = after.strip_prefix('/').unwrap_or("");
                (Target::Transfer(name.to_string()), tags)
            }
            None => (Target::Category(s.to_string()), ""),
        }
    } else {
        let (cat, tags) = s.split_once('/').unwrap_or((s, ""));
        let cat = cat.trim();
        let t = if cat.is_empty() {
            Target::None
        } else {
            Target::Category(cat.to_string())
        };
        (t, tags)
    };
    let tags = tags
        .split(':')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();
    let head = match head {
        Target::Transfer(n) if n.is_empty() => Target::None,
        other => other,
    };
    (head, tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    #[test]
    fn reads_quicken_dates() {
        let m = DateOrder::Mdy;
        assert_eq!(read_date("1/5'26", m).unwrap(), d("2026-01-05"));
        assert_eq!(read_date("1/ 5'26", m).unwrap(), d("2026-01-05"));
        assert_eq!(read_date("12/31/98", m).unwrap(), d("1998-12-31"));
        assert_eq!(read_date("1/5/2026", m).unwrap(), d("2026-01-05"));
        assert_eq!(
            read_date("2026-01-05", DateOrder::Dmy).unwrap(),
            d("2026-01-05")
        );
        assert_eq!(
            read_date("5/1'26", DateOrder::Dmy).unwrap(),
            d("2026-01-05")
        );
        assert_eq!(
            read_date("31.12.98", DateOrder::Dmy).unwrap(),
            d("1998-12-31")
        );
        assert!(read_date("2/30'26", m).is_err());
        assert!(read_date("13/1'26", m).is_err());
        assert!(read_date("", m).is_err());
        assert!(read_date("1/5", m).is_err());
        assert!(read_date("a/5/26", m).is_err());
    }

    #[test]
    fn reads_amounts_and_fractions() {
        assert_eq!(money("-1,234.56").unwrap().cents(), -123_456);
        assert_eq!(money("1234").unwrap().cents(), 123_400);
        assert_eq!(money("0.125").unwrap().cents(), 12);
        assert_eq!(money("0.135").unwrap().cents(), 14);
        assert!(money("abc").is_err());
        assert_eq!(scaled6("12 1/2").unwrap(), 12_500_000);
        assert_eq!(scaled6("-2 3/8").unwrap(), -2_375_000);
        assert_eq!(scaled6("3/8").unwrap(), 375_000);
        assert_eq!(scaled6("1.2345678").unwrap(), 1_234_568);
        assert!(scaled6("1/0").is_err());
    }

    #[test]
    fn decodes_windows_1252() {
        assert_eq!(decode(b"Caf\xe9 \x80 \x93q\x94"), "Café € “q”");
        assert_eq!(decode("Café".as_bytes()), "Café");
        assert_eq!(decode(b"\xEF\xBB\xBFabc"), "abc");
    }

    #[test]
    fn splits_categories_transfers_and_tags() {
        assert_eq!(
            split_target("Food:Dining/Vacation"),
            (
                Target::Category("Food:Dining".into()),
                vec!["Vacation".into()]
            )
        );
        assert_eq!(
            split_target("[Savings]/Trip:Kids"),
            (
                Target::Transfer("Savings".into()),
                vec!["Trip".into(), "Kids".into()]
            )
        );
        assert_eq!(split_target("/Tag"), (Target::None, vec!["Tag".into()]));
        assert_eq!(split_target(""), (Target::None, vec![]));
        assert_eq!(split_target("[]"), (Target::None, vec![]));
    }

    const WHOLE: &str = "!Type:Tag
NVacation
^
!Type:Cat
NFood:Dining
DEating out
E
^
NSalary
I
T
^
!Option:AutoSwitch
!Account
NChecking
TBank
^
NVisa
TCCard
L5,000.00
^
!Clear:AutoSwitch
!Account
NChecking
TBank
^
!Type:Bank
D1/ 5'26
T-1,234.56
CX
N101
P**VOID**Store
MHello
LFood:Dining/Vacation
^
D1/6'26
T-100.00
PSplit
SFood:Dining
EDinner
$-60.00
S[Visa]
$-40.00
^
!Type:Memorized
KC
T-5.00
^
!Type:Security
NVanguard Total
SVTI
TMutual Fund
^
!Type:Prices
\"VTI\",12 1/2,\" 1/ 5'26\"
\"VTI\",,\"1/6'26\"
^
!Account
NBrokerage
TInvst
^
!Type:Invst
D1/7'26
NBuyX
YVanguard Total
I12.5
Q10
T125.00
O0
L[Checking]
$125.00
^
!Type:Budget
X1
^
";

    #[test]
    fn reads_a_whole_file_export() {
        let f = parse(WHOLE, "x", None);
        assert_eq!(f.accounts.len(), 3);
        assert_eq!(f.accounts[0].name, "Checking");
        assert_eq!(f.accounts[0].kind, Some(AccountKind::Bank));
        assert_eq!(f.accounts[1].credit_limit.unwrap().cents(), 500_000);
        assert_eq!(f.accounts[2].kind, Some(AccountKind::Investment));
        assert_eq!(f.tags[0].name, "Vacation");
        assert_eq!(f.categories.len(), 2);
        assert_eq!(f.categories[0].income, Some(false));
        assert!(f.categories[1].tax_related);
        assert_eq!(f.memorized, 1);
        assert_eq!(f.securities[0].symbol.as_deref(), Some("VTI"));
        assert_eq!(f.prices.len(), 2);
        assert_eq!(f.prices[0].price, Some(Price::from_raw(12_500_000)));
        assert_eq!(f.prices[0].date, Some(d("2026-01-05")));
        assert_eq!(f.prices[1].price, None);

        assert_eq!(f.bank.len(), 2);
        let r = &f.bank[0];
        assert_eq!(r.account, 0);
        assert_eq!(r.date, Some(d("2026-01-05")));
        assert_eq!(r.amount.unwrap().cents(), -123_456);
        assert_eq!(r.cleared, Cleared::Reconciled);
        assert!(r.void);
        assert_eq!(r.payee, "Store");
        assert_eq!(r.num, "101");
        let s = &f.bank[1];
        assert_eq!(s.splits.len(), 2);
        assert_eq!(s.splits[0].memo, "Dinner");
        assert_eq!(s.splits[1].category, "[Visa]");
        assert_eq!(s.splits[1].amount.unwrap().cents(), -4000);

        assert_eq!(f.invest.len(), 1);
        let i = &f.invest[0];
        assert_eq!(i.account, 2);
        assert_eq!(i.action, "BuyX");
        assert_eq!(i.quantity, Some(Quantity::from_raw(10_000_000)));
        assert_eq!(i.transfer_amount.unwrap().cents(), 12_500);
        assert_eq!(i.category, "[Checking]");
        assert!(f.notes.iter().any(|n| n.message.contains("Budget")));
        assert_eq!(f.date_order, DateOrder::Mdy);
    }

    #[test]
    fn per_account_export_uses_the_default_account() {
        let f = parse("!Type:CCard\nD1/5'26\nT-5.00\nPShop\n^\n", "My Visa", None);
        assert_eq!(f.accounts.len(), 1);
        assert_eq!(f.accounts[0].name, "My Visa");
        assert_eq!(f.accounts[0].kind, Some(AccountKind::CreditCard));
        assert!(!f.accounts[0].defined);
        assert_eq!(f.bank[0].account, 0);
    }

    #[test]
    fn decides_day_first_from_the_file() {
        let text = "!Type:Bank\nD25/12'25\nT1\n^\nD3/4'26\nT1\n^\n";
        let f = parse(text, "a", None);
        assert_eq!(f.date_order, DateOrder::Dmy);
        assert!(!f.date_ambiguous);
        assert_eq!(f.bank[1].date, Some(d("2026-04-03")));
        let f = parse("!Type:Bank\nD3/4'26\nT1\n^\n", "a", None);
        assert!(f.date_ambiguous);
        assert_eq!(f.bank[0].date, Some(d("2026-03-04")));
        let f = parse("!Type:Bank\nD3/4'26\nT1\n^\n", "a", Some(DateOrder::Dmy));
        assert!(!f.date_ambiguous);
        assert_eq!(f.bank[0].date, Some(d("2026-04-03")));
    }

    #[test]
    fn bad_fields_stay_on_their_record() {
        let f = parse("!Type:Bank\nD2/30'26\nTabc\n^\n", "a", None);
        let r = &f.bank[0];
        assert_eq!(r.problems.len(), 2, "{:?}", r.problems);
        assert_eq!(r.date, None);
        assert_eq!(r.amount, None);
    }
}
