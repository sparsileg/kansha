//! Transactions as the category, payee, and tax reports read them: each
//! with its postings and the names they need, plus the lines a report
//! lists (a category posting, or one side of a transfer).

use std::cmp::Ordering;
use std::collections::HashMap;

use rusqlite::Connection;

use super::{DetailSort, Drill, ReportSettings, ResolvedRange};
use crate::accounts::{Account, AccountId, TaxTreatment};
use crate::categories::{Category, CategoryId, CategoryKind, PayeeId, TagId, TaxLine, TaxLineId};
use crate::date::Date;
use crate::error::Result;
use crate::invest::InvAction;
use crate::ledger::{Cleared, TxnId};
use crate::money::{Money, Quantity};
use crate::persistence::reports::PostingRow;
use crate::persistence::{accounts, categories, reports as repo, securities, tags};
use crate::securities::{Security, SecurityId};

/// Names and attributes the reports look up.
pub(super) struct Lookups {
    pub accounts: Vec<Account>,
    account_index: HashMap<AccountId, usize>,
    pub categories: Vec<Category>,
    category_index: HashMap<CategoryId, usize>,
    paths: HashMap<CategoryId, String>,
    tags: HashMap<TagId, String>,
    securities: HashMap<SecurityId, Security>,
    pub tax_lines: Vec<TaxLine>,
}

impl Lookups {
    pub fn load(conn: &Connection) -> Result<Lookups> {
        let accounts = accounts::list(conn)?;
        let categories = categories::list(conn)?;
        let account_index = accounts
            .iter()
            .enumerate()
            .map(|(i, a)| (a.id, i))
            .collect();
        let category_index: HashMap<CategoryId, usize> = categories
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id, i))
            .collect();
        let mut paths = HashMap::new();
        for c in &categories {
            let mut names = vec![c.fields.name.clone()];
            let mut parent = c.fields.parent;
            while let Some(p) = parent {
                let Some(pc) = category_index.get(&p).map(|&i| &categories[i]) else {
                    break;
                };
                names.push(pc.fields.name.clone());
                parent = pc.fields.parent;
            }
            names.reverse();
            paths.insert(c.id, names.join(":"));
        }
        Ok(Lookups {
            accounts,
            account_index,
            categories,
            category_index,
            paths,
            tags: tags::list(conn)?
                .into_iter()
                .map(|t| (t.id, t.fields.name))
                .collect(),
            securities: securities::list(conn)?
                .into_iter()
                .map(|s| (s.id, s))
                .collect(),
            tax_lines: repo::tax_lines(conn)?,
        })
    }

    pub fn account(&self, id: AccountId) -> Option<&Account> {
        self.account_index.get(&id).map(|&i| &self.accounts[i])
    }

    pub fn account_name(&self, id: AccountId) -> String {
        self.account(id)
            .map_or_else(|| format!("#{}", id.0), |a| a.fields.name.clone())
    }

    /// Position in the account list's order.
    pub fn account_order(&self, id: AccountId) -> usize {
        self.account_index.get(&id).copied().unwrap_or(usize::MAX)
    }

    pub fn taxable(&self, id: AccountId) -> bool {
        self.account(id)
            .is_some_and(|a| a.fields.tax_treatment == TaxTreatment::Taxable)
    }

    pub fn category(&self, id: CategoryId) -> Option<&Category> {
        self.category_index.get(&id).map(|&i| &self.categories[i])
    }

    pub fn category_path(&self, id: CategoryId) -> String {
        self.paths.get(&id).cloned().unwrap_or_default()
    }

    pub fn tag_names(&self, ids: &[TagId]) -> String {
        ids.iter()
            .filter_map(|t| self.tags.get(t).map(String::as_str))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// A security's name (the Capital Gains and investment rows show
    /// names, as Quicken does).
    pub fn security_name(&self, id: SecurityId) -> String {
        self.securities
            .get(&id)
            .map_or_else(String::new, |s| s.fields.name.clone())
    }

    pub fn tax_line(&self, id: TaxLineId) -> Option<&TaxLine> {
        self.tax_lines.iter().find(|t| t.id == id)
    }

    /// A form's place among the forms: its first line's order.
    pub fn form_order(&self, tl: &TaxLine) -> i64 {
        self.tax_lines
            .iter()
            .filter(|x| x.form == tl.form)
            .map(|x| x.sort_order)
            .min()
            .unwrap_or(tl.sort_order)
    }

    pub fn tax_label(&self, id: Option<TaxLineId>) -> String {
        id.and_then(|t| self.tax_line(t))
            .map_or_else(String::new, TaxLine::label)
    }

    /// The tax line of a transfer posting to `account`: money leaving it
    /// (`amount` negative) or coming in.
    pub fn transfer_tax_line(&self, account: AccountId, amount: Money) -> Option<TaxLineId> {
        let a = self.account(account)?;
        if amount.is_negative() {
            a.fields.tax_line_out
        } else if amount.is_zero() {
            None
        } else {
            a.fields.tax_line_in
        }
    }
}

/// Investment detail of a transaction.
pub(super) struct Inv {
    pub account: AccountId,
    pub action: InvAction,
    pub quantity: Option<Quantity>,
    pub security: Option<SecurityId>,
    pub to_account: Option<AccountId>,
    pub conversion: Option<crate::invest::ConversionTax>,
}

/// One transaction with its postings in line order.
pub(super) struct TxnFacts {
    pub id: TxnId,
    pub date: Date,
    pub payee: Option<crate::categories::PayeeId>,
    pub payee_name: String,
    pub check_num: String,
    pub memo: String,
    pub inv: Option<Inv>,
    pub postings: Vec<PostingRow>,
}

/// Normal transactions in the range, oldest first.
pub(super) fn load(conn: &Connection, range: ResolvedRange) -> Result<Vec<TxnFacts>> {
    let mut out: Vec<TxnFacts> = Vec::new();
    for p in repo::postings(conn, range.from, range.to)? {
        if out.last().is_none_or(|t| t.id != p.txn) {
            out.push(TxnFacts {
                id: p.txn,
                date: p.date,
                payee: p.payee,
                payee_name: p.payee_name.clone(),
                check_num: p.check_num.clone(),
                memo: p.txn_memo.clone(),
                inv: match (p.inv_account, p.inv_action) {
                    (Some(account), Some(action)) => Some(Inv {
                        account,
                        action,
                        quantity: p.inv_quantity,
                        security: p.inv_security,
                        to_account: p.inv_to_account,
                        conversion: p.inv_conversion,
                    }),
                    _ => None,
                },
                postings: Vec::new(),
            });
        }
        if let Some(t) = out.last_mut() {
            t.postings.push(p);
        }
    }
    Ok(out)
}

impl TxnFacts {
    /// Cash postings to accounts (not a holding's basis), in line order.
    pub fn account_postings(&self) -> impl Iterator<Item = (usize, AccountId, &PostingRow)> {
        self.postings
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match (p.account, p.security) {
                (Some(a), None) => Some((i, a, p)),
                _ => None,
            })
    }

    /// The account a category line belongs to, if the filter includes
    /// it: the account a banking entry was written in; the investment
    /// account of a trade whose cash is in a linked account (the cash is
    /// a transfer, see [`TxnFacts::linked_cash`]); otherwise for an
    /// investment transaction its account, else the account its cash
    /// went to.
    pub fn home(&self, s: &ReportSettings) -> Option<AccountId> {
        let Some(inv) = &self.inv else {
            return self.main().map(|(_, a)| a).filter(|a| s.account_ok(*a));
        };
        let own = Some(inv.account).filter(|a| s.account_ok(*a));
        if self.linked_cash().is_some() {
            return own;
        }
        own.or_else(|| {
            self.postings
                .iter()
                .filter_map(|p| p.account)
                .find(|a| s.account_ok(*a))
        })
    }

    /// A trade whose cash is in a linked account (INV-300): the cash
    /// posting's index and account. Its holding changes in the
    /// investment account, so the cash moves between the two.
    pub fn linked_cash(&self) -> Option<(usize, AccountId)> {
        let inv = self.inv.as_ref()?;
        let holding = self
            .postings
            .iter()
            .any(|p| p.account == Some(inv.account) && p.security.is_some());
        if !holding {
            return None;
        }
        self.account_postings()
            .find(|(_, a, _)| *a != inv.account)
            .map(|(i, a, _)| (i, a))
    }

    /// The main account (the register the entry was written in): the
    /// first cash account posting.
    pub fn main(&self) -> Option<(usize, AccountId)> {
        self.account_postings().next().map(|(i, a, _)| (i, a))
    }

    pub fn payee_ok(&self, s: &ReportSettings) -> bool {
        match (&s.payees, self.payee) {
            (None, _) => true,
            (Some(ids), Some(p)) => ids.contains(&p),
            (Some(_), None) => false,
        }
    }

    /// The Num column: the investment action, or the check number.
    pub fn num(&self) -> String {
        self.inv
            .as_ref()
            .map_or_else(|| self.check_num.clone(), |i| i.action.label().to_string())
    }

    /// The Description column: the payee, or for an investment
    /// transaction without one, its shares and security.
    pub fn description(&self, lk: &Lookups) -> String {
        if !self.payee_name.is_empty() {
            return self.payee_name.clone();
        }
        match &self.inv {
            Some(Inv {
                security: Some(s),
                quantity,
                ..
            }) => {
                let name = lk.security_name(*s);
                match quantity {
                    Some(q) => format!("{q} {name}"),
                    None => name,
                }
            }
            _ => String::new(),
        }
    }

    /// A split (the S column): more than one category or transfer line
    /// besides the main account's; a holding's basis is not a line.
    pub fn is_split(&self) -> bool {
        let main = self.main().map(|(i, _)| i);
        self.postings
            .iter()
            .enumerate()
            .filter(|(i, p)| Some(*i) != main && p.security.is_none())
            .count()
            > 1
    }

    /// The Clr column for `account`'s posting.
    pub fn clr(&self, account: AccountId) -> String {
        let cleared = self
            .postings
            .iter()
            .find(|p| p.account == Some(account) && p.security.is_none())
            .map_or(Cleared::Unmarked, |p| p.cleared);
        match cleared {
            Cleared::Reconciled => "R".into(),
            Cleared::Cleared => "c".into(),
            Cleared::Unmarked => String::new(),
        }
    }

    /// A line's tags: its own plus the transaction-level tags, which
    /// live on the main account's posting (TAG-010).
    pub fn tags_for(&self, p: &PostingRow) -> Vec<TagId> {
        let mut tags = p.tags.clone();
        if let Some((i, _)) = self.main() {
            tags.extend(self.postings[i].tags.iter().copied());
        }
        tags.sort();
        tags.dedup();
        tags
    }

    fn memo_of(&self, p: &PostingRow) -> String {
        if p.memo.is_empty() {
            self.memo.clone()
        } else {
            p.memo.clone()
        }
    }
}

/// Where a line is listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Section {
    Income,
    Expenses,
    Transfers,
}

impl Section {
    pub fn label(self) -> &'static str {
        match self {
            Section::Income => "INCOME",
            Section::Expenses => "EXPENSES",
            Section::Transfers => "TRANSFERS",
        }
    }

    pub fn of(kind: CategoryKind) -> Option<Section> {
        match kind {
            CategoryKind::Income => Some(Section::Income),
            CategoryKind::Expense => Some(Section::Expenses),
            CategoryKind::Equity => None,
        }
    }
}

/// What a line is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    Category(CategoryId),
    /// The other account of a transfer.
    Transfer(AccountId),
}

/// One report line: a transaction's category posting or one side of a
/// transfer, in report sign.
pub(super) struct Line {
    pub section: Section,
    pub target: Target,
    pub txn: TxnId,
    pub line_no: i64,
    pub date: Date,
    pub account: AccountId,
    pub payee: Option<PayeeId>,
    pub payee_name: String,
    pub num: String,
    pub description: String,
    pub memo: String,
    /// "Parent:Child", or "[Account]" for a transfer.
    pub category: String,
    pub tag: String,
    pub tax_line: Option<TaxLineId>,
    pub clr: String,
    /// The transaction is a split ([`TxnFacts::is_split`]).
    pub split: bool,
    pub amount: Money,
}

impl Line {
    pub fn drill(&self) -> Drill {
        Drill::Txn {
            account: self.account,
            txn: self.txn,
            date: self.date,
        }
    }
}

fn tags_ok(s: &ReportSettings, tags: &[TagId]) -> bool {
    s.tags
        .as_ref()
        .is_none_or(|ids| tags.iter().any(|t| ids.contains(t)))
}

/// Which lines a report wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Want {
    /// Every category line; transfers when the settings say so.
    All,
    /// Tax Summary: tax-related or tax-line categories and tax-line
    /// transfers, from taxable accounts.
    TaxRelated,
    /// Tax Schedule: lines with a tax line, from taxable accounts.
    TaxLines,
}

/// The lines of `txns` a report lists.
pub(super) fn lines(
    txns: &[TxnFacts],
    s: &ReportSettings,
    lk: &Lookups,
    want: Want,
) -> Result<Vec<Line>> {
    let mut out = Vec::new();
    for t in txns {
        if !t.payee_ok(s) {
            continue;
        }
        if want != Want::All
            && let Some(inv) = t
                .inv
                .as_ref()
                .filter(|i| i.action == InvAction::RothConversion)
        {
            out.extend(conversion_lines(t, inv, s, lk)?);
            continue;
        }
        let description = t.description(lk);
        let num = t.num();
        let split = t.is_split();
        let base = |account: AccountId, p: &PostingRow, section, target, category: String| Line {
            section,
            target,
            txn: t.id,
            line_no: p.line_no,
            date: t.date,
            account,
            payee: t.payee,
            payee_name: t.payee_name.clone(),
            num: num.clone(),
            description: description.clone(),
            memo: t.memo_of(p),
            category,
            tag: lk.tag_names(&t.tags_for(p)),
            tax_line: None,
            clr: t.clr(account),
            split,
            amount: Money::ZERO,
        };

        // Category lines.
        if let Some(home) = t.home(s) {
            let tax_ok = want == Want::All || lk.taxable(home);
            for p in &t.postings {
                let Some(cid) = p.category else { continue };
                let Some(cat) = lk.category(cid) else {
                    continue;
                };
                let Some(section) = Section::of(cat.fields.kind) else {
                    continue;
                };
                if !tax_ok
                    || !ReportSettings::includes(&s.categories, &cid)
                    || !tags_ok(s, &t.tags_for(p))
                {
                    continue;
                }
                let realized = cat.system == Some(crate::categories::SystemCategory::RealizedGain);
                let keep = match want {
                    Want::All => true,
                    Want::TaxRelated => cat.fields.tax_related || cat.fields.tax_line.is_some(),
                    // Schedule D comes from the lots, not the category.
                    Want::TaxLines => cat.fields.tax_line.is_some() && !realized,
                };
                if !keep {
                    continue;
                }
                let mut line = base(
                    home,
                    p,
                    section,
                    Target::Category(cid),
                    lk.category_path(cid),
                );
                line.tax_line = cat.fields.tax_line;
                line.amount = p
                    .amount
                    .checked_neg()
                    .ok_or(crate::Error::Overflow("report"))?;
                out.push(line);
            }
        }

        // Transfer lines.
        let cash: Vec<(usize, AccountId)> = t.account_postings().map(|(i, a, _)| (i, a)).collect();
        match want {
            Want::All => {
                if !s.transfers {
                    continue;
                }
                // A linked-cash trade: from the cash account to the
                // investment account (or back).
                if let (Some((i, cash_acct)), Some(inv)) = (t.linked_cash(), &t.inv) {
                    let p = &t.postings[i];
                    if tags_ok(s, &t.tags_for(p)) {
                        if s.account_ok(cash_acct) {
                            let mut line = base(
                                cash_acct,
                                p,
                                Section::Transfers,
                                Target::Transfer(inv.account),
                                format!("[{}]", lk.account_name(inv.account)),
                            );
                            line.amount = p.amount;
                            out.push(line);
                        }
                        if s.account_ok(inv.account) {
                            let mut line = base(
                                inv.account,
                                p,
                                Section::Transfers,
                                Target::Transfer(cash_acct),
                                format!("[{}]", lk.account_name(cash_acct)),
                            );
                            line.amount = p
                                .amount
                                .checked_neg()
                                .ok_or(crate::Error::Overflow("report"))?;
                            out.push(line);
                        }
                    }
                    continue;
                }
                let Some((_, main)) = t.main() else {
                    continue;
                };
                for &(i, other) in &cash {
                    let p = &t.postings[i];
                    if other == main || !tags_ok(s, &t.tags_for(p)) {
                        continue;
                    }
                    // Seen from the main account: the money that went to
                    // (or came from) the other one.
                    if s.account_ok(main) {
                        let mut line = base(
                            main,
                            p,
                            Section::Transfers,
                            Target::Transfer(other),
                            format!("[{}]", lk.account_name(other)),
                        );
                        line.amount = p
                            .amount
                            .checked_neg()
                            .ok_or(crate::Error::Overflow("report"))?;
                        out.push(line);
                    }
                    // Seen from the other account.
                    if s.account_ok(other) {
                        let mut line = base(
                            other,
                            p,
                            Section::Transfers,
                            Target::Transfer(main),
                            format!("[{}]", lk.account_name(main)),
                        );
                        line.amount = p.amount;
                        line.clr = t.clr(other);
                        out.push(line);
                    }
                }
            }
            Want::TaxRelated | Want::TaxLines => {
                for &(i, q) in &cash {
                    let p = &t.postings[i];
                    let Some(tax_line) = lk.transfer_tax_line(q, p.amount) else {
                        continue;
                    };
                    // The other side: the main account, or for the main
                    // account itself the first other one.
                    let Some(&(_, side)) = cash
                        .iter()
                        .find(|(_, a)| *a != q && Some(*a) == t.main().map(|m| m.1))
                        .or_else(|| cash.iter().find(|(_, a)| *a != q))
                    else {
                        continue;
                    };
                    if !s.account_ok(side) {
                        continue;
                    }
                    let mut line = base(
                        side,
                        p,
                        Section::Transfers,
                        Target::Transfer(q),
                        format!("[{}]", lk.account_name(q)),
                    );
                    line.tax_line = Some(tax_line);
                    line.amount = p
                        .amount
                        .checked_neg()
                        .ok_or(crate::Error::Overflow("report"))?;
                    out.push(line);
                }
            }
        }
    }
    Ok(out)
}

/// Order lines inside a group.
/// Order lines by `sort`, reversed when `desc`; ties stay in date and
/// entry order either way.
pub(super) fn sort_lines(lines: &mut [&Line], sort: DetailSort, desc: bool, lk: &Lookups) {
    let primary = |a: &Line, b: &Line| -> Ordering {
        let account = |l: &Line| lk.account_order(l.account);
        match sort {
            DetailSort::Date => (a.date, account(a)).cmp(&(b.date, account(b))),
            DetailSort::AccountDate => (account(a), a.date).cmp(&(account(b), b.date)),
            DetailSort::Amount => a.amount.cmp(&b.amount),
            DetailSort::Num => num_key(&a.num).cmp(&num_key(&b.num)),
        }
    };
    lines.sort_by(|a, b| {
        let first = primary(a, b);
        let first = if desc { first.reverse() } else { first };
        first.then_with(|| (a.date, a.txn, a.line_no).cmp(&(b.date, b.txn, b.line_no)))
    });
}

/// Check numbers sort as numbers, then other text (ignoring case), then
/// empty.
fn num_key(num: &str) -> (u8, u64, String) {
    let num = num.trim();
    if num.is_empty() {
        return (2, 0, String::new());
    }
    match num.parse::<u64>() {
        Ok(n) => (0, n, String::new()),
        Err(_) => (1, 0, num.to_lowercase()),
    }
}

/// A line's cell for a column ID.
/// A Roth conversion's tax lines (INV-070), on 1099-R whatever the
/// account's transfer settings: the taxable part (value converted plus
/// tax withheld, less the nontaxable part) as an IRA or, from a 401(k),
/// a pension distribution, shown as a transfer to the Roth IRA; and the
/// federal and state tax withheld.
fn conversion_lines(
    t: &TxnFacts,
    inv: &Inv,
    s: &ReportSettings,
    lk: &Lookups,
) -> Result<Vec<Line>> {
    let (Some(roth), true) = (inv.to_account, s.account_ok(inv.account)) else {
        return Ok(Vec::new());
    };
    let pension = lk
        .account(inv.account)
        .is_some_and(|a| a.fields.account_type == crate::accounts::AccountType::Retirement401k);
    let [total, federal, state] = if pension {
        [
            "Total pension taxable distrib.",
            "Pension federal tax withheld",
            "Pension state tax withheld",
        ]
    } else {
        [
            "Total IRA taxable distrib.",
            "IRA federal tax withheld",
            "IRA state tax withheld",
        ]
    };
    let tax_line = |line: &str| -> Option<TaxLineId> {
        lk.tax_lines
            .iter()
            .find(|x| x.form == "1099-R" && x.line == line)
            .map(|x| x.id)
    };
    let c = inv.conversion.unwrap_or_default();
    let overflow = || crate::Error::Overflow("report");
    // The value converted: shares or cash the Roth IRA side receives.
    let converted: Money = t
        .postings
        .iter()
        .filter(|p| match inv.security {
            Some(sec) => p.account == Some(roth) && p.security == Some(sec),
            None => p.account.is_some() && p.security.is_none() && p.amount.cents() > 0,
        })
        .map(|p| p.amount)
        .sum();
    let taxable = converted
        .checked_add(c.withheld_federal)
        .and_then(|v| v.checked_add(c.withheld_state))
        .and_then(|v| v.checked_sub(c.nontaxable))
        .ok_or_else(overflow)?;
    let Some(first) = t.postings.first() else {
        return Ok(Vec::new());
    };
    let line = |section, target, category: String, tax_line, amount| Line {
        section,
        target,
        txn: t.id,
        line_no: first.line_no,
        date: t.date,
        account: inv.account,
        payee: t.payee,
        payee_name: t.payee_name.clone(),
        num: t.num(),
        description: t.description(lk),
        memo: t.memo.clone(),
        category,
        tag: lk.tag_names(&t.tags_for(first)),
        tax_line,
        clr: t.clr(inv.account),
        split: false,
        amount,
    };
    let mut out = Vec::new();
    if !taxable.is_zero() {
        out.push(line(
            Section::Transfers,
            Target::Transfer(roth),
            format!("[{}]", lk.account_name(roth)),
            tax_line(total),
            taxable,
        ));
    }
    let withheld = lk
        .categories
        .iter()
        .find(|x| x.system == Some(crate::categories::SystemCategory::TaxWithheld));
    if let Some(cat) = withheld {
        for (amount, name) in [(c.withheld_federal, federal), (c.withheld_state, state)] {
            if !amount.is_zero() {
                out.push(line(
                    Section::Expenses,
                    Target::Category(cat.id),
                    lk.category_path(cat.id),
                    tax_line(name),
                    amount.checked_neg().ok_or_else(overflow)?,
                ));
            }
        }
    }
    Ok(out)
}

pub(super) fn cell(l: &Line, id: &str, lk: &Lookups) -> String {
    match id {
        "date" => l.date.to_string(),
        "account" => lk.account_name(l.account),
        "num" => l.num.clone(),
        "description" | "payee" => l.description.clone(),
        "memo" => l.memo.clone(),
        "category" => l.category.clone(),
        "tag" => l.tag.clone(),
        "tax_item" => lk.tax_label(l.tax_line),
        "clr" => l.clr.clone(),
        "split" => if l.split { "S" } else { "" }.to_string(),
        "amount" => l.amount.to_string(),
        _ => String::new(),
    }
}
