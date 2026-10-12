//! Find/Replace (REG-170): find register rows by field and comparison,
//! then change one field on the rows the user checked.
//!
//! A found row is a transaction as one account's register shows it, or,
//! with "within splits", one line of a split. Voided transactions and
//! any transaction touching an investment account are never found: the
//! investment register owns those, and a voided one cannot be edited.
//!
//! Every comparison applies to every field. Amount and Date compare as
//! numbers and dates for Exactly, Greater, and Less, and as the text the
//! register shows for Contains, Starts with, and Ends with. A check
//! number compares as a number when both sides are whole numbers. Any
//! other field compares as text, ignoring case, alphabetically.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::{Cleared, PostingInput, Target, TxnId, TxnInput, TxnStatus};
use crate::accounts::{AccountId, AccountStatus};
use crate::categories::{CategoryId, CategoryKind, TagFields, TagId};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::Money;
use crate::persistence::{Tx, accounts, categories, ledger as repo, payees, tags};
use crate::settings::{self, DateFormat};
use crate::text_enum::text_enum;
use crate::undo::Created;

text_enum! {
    /// What a Find row looks at, and what Replace changes (REG-170).
    pub enum FindField {
        Amount = "amount",
        Category = "category",
        CheckNum = "check_num",
        Cleared = "cleared",
        Date = "date",
        Memo = "memo",
        Payee = "payee",
        Tag = "tag",
    }
}

text_enum! {
    /// How a Find row compares its field with its value (REG-170).
    pub enum FindComparison {
        Contains = "contains",
        Exactly = "exactly",
        StartsWith = "starts_with",
        EndsWith = "ends_with",
        Greater = "greater",
        GreaterOrEqual = "greater_or_equal",
        Less = "less",
        LessOrEqual = "less_or_equal",
    }
}

text_enum! {
    /// How several Find rows combine: one operator for all of them.
    pub enum FindJoin {
        And = "and",
        Or = "or",
    }
}

text_enum! {
    /// Found list columns that sort. Ties go newest first.
    pub enum FindSort {
        Date = "date",
        Account = "account",
        CheckNum = "check_num",
        Payee = "payee",
        Category = "category",
        Tag = "tag",
        Memo = "memo",
        Cleared = "cleared",
        Amount = "amount",
    }
}

/// One Find row. A blank value leaves the row out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FindCondition {
    pub field: FindField,
    pub comparison: FindComparison,
    /// As typed. An Amount for Exactly, Greater, or Less is a number
    /// ("-16.46"); a Date for those is ISO ("2026-03-05").
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FindQuery {
    pub conditions: Vec<FindCondition>,
    pub join: FindJoin,
    /// Show matches within splits: each line of a split is a row of its
    /// own, matched on its category, memo, amount, and tags.
    pub within_splits: bool,
    pub sort: FindSort,
    pub descending: bool,
}

/// Which row: a transaction in one account's register, or one line of
/// its split (`line`: the posting's line number).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FoundKey {
    pub txn: TxnId,
    pub account: AccountId,
    pub line: Option<i64>,
}

/// One row of the Found list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FoundRow {
    pub key: FoundKey,
    pub date: Date,
    pub account_name: String,
    pub check_num: String,
    pub payee: String,
    /// A category path, `[Account]` for a transfer, `--Split--`, or
    /// empty; a split line's own category.
    pub category: String,
    /// Tag names, ", " between.
    pub tags: String,
    pub memo: String,
    /// This account's posting.
    pub cleared: Cleared,
    /// This account's posting; a split line's amount, register sign.
    pub amount: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FindResult {
    pub rows: Vec<FoundRow>,
    /// Transactions among the rows ("Found in N transactions").
    pub txns: usize,
}

/// What Replace All sets on the checked rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ReplaceSpec {
    pub field: FindField,
    /// An Amount is a number or sum (REG-035); a Date is ISO; Cleared
    /// is "unmarked" or "cleared". A new category, payee, or tag name
    /// is created.
    pub with: String,
}

/// What Replace All did, counted by transaction.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ReplaceResult {
    pub changed: usize,
    /// Already had the value.
    pub unchanged: usize,
    /// Amount on a split, or a split line.
    pub skipped_split: usize,
    /// Amount or Category on a transfer, or a row with no category line.
    pub skipped_transfer: usize,
    /// In a closed account (ACCT-210).
    pub skipped_closed: usize,
    /// Voided or deleted since the Find.
    pub skipped_gone: usize,
}

/// A posting as Find sees it (read by `persistence::ledger`).
#[derive(Debug, Clone)]
pub struct FindPosting {
    pub txn: TxnId,
    pub line_no: i64,
    pub account: Option<AccountId>,
    pub category: Option<CategoryId>,
    /// The category's path; empty for an account posting.
    pub path: String,
    pub amount: Money,
    pub memo: String,
    pub cleared: Cleared,
    /// Tag names.
    pub tags: Vec<String>,
    pub date: Date,
    pub check_num: String,
    pub txn_memo: String,
    pub payee: String,
}

/// A row before matching: what each field holds.
struct Candidate {
    row: FoundRow,
    tag_names: Vec<String>,
}

/// Rows matching `query` (REG-170), sorted.
pub fn find(conn: &Connection, query: &FindQuery) -> Result<FindResult> {
    let conditions: Vec<Prepared> = query
        .conditions
        .iter()
        .filter(|c| !c.value.trim().is_empty())
        .map(Prepared::new)
        .collect::<Result<_>>()?;
    if conditions.is_empty() {
        return Ok(FindResult {
            rows: Vec::new(),
            txns: 0,
        });
    }
    let format = settings::load(conn)?.date_format;
    let names: HashMap<AccountId, (String, bool)> = accounts::list(conn)?
        .into_iter()
        .map(|a| {
            let invest = a.fields.account_type.is_investment();
            (a.id, (a.fields.name, invest))
        })
        .collect();
    let postings = repo::find_postings(conn)?;
    let mut rows = Vec::new();
    for group in postings.chunk_by(|a, b| a.txn == b.txn) {
        for c in candidates(group, &names, query.within_splits) {
            let hit = |p: &Prepared| p.matches(&c, format);
            let keep = match query.join {
                FindJoin::And => conditions.iter().all(hit),
                FindJoin::Or => conditions.iter().any(hit),
            };
            if keep {
                rows.push(c.row);
            }
        }
    }
    sort(&mut rows, query.sort, query.descending);
    let txns = rows.iter().map(|r| r.key.txn).collect::<HashSet<_>>().len();
    Ok(FindResult { rows, txns })
}

/// The register rows of one transaction (its postings in line order),
/// and with `within_splits` its split lines. None when it touches an
/// investment account.
fn candidates(
    postings: &[FindPosting],
    names: &HashMap<AccountId, (String, bool)>,
    within_splits: bool,
) -> Vec<Candidate> {
    let name_of = |a: AccountId| names.get(&a).map_or("", |(n, _)| n.as_str());
    if postings.iter().any(|p| {
        p.account
            .is_some_and(|a| names.get(&a).is_none_or(|(_, inv)| *inv))
    }) {
        return Vec::new();
    }
    let target_text = |p: &FindPosting| match p.account {
        Some(a) => format!("[{}]", name_of(a)),
        None => p.path.clone(),
    };
    let mut out = Vec::new();
    for (i, main) in postings.iter().enumerate() {
        let Some(account) = main.account else {
            continue;
        };
        let others: Vec<&FindPosting> = postings
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, p)| p)
            .collect();
        let category = match others.as_slice() {
            [] => String::new(),
            [one] => target_text(one),
            _ => "--Split--".to_string(),
        };
        // As the register's Tag column: a split shows only its own tags.
        let mut tag_names: Vec<String> = if others.len() > 1 {
            main.tags.clone()
        } else {
            postings.iter().flat_map(|p| p.tags.clone()).collect()
        };
        tag_names.sort_by_key(|t| t.to_lowercase());
        tag_names.dedup();
        let row =
            |line: Option<i64>, category: String, tag_names: Vec<String>, memo, amount| Candidate {
                row: FoundRow {
                    key: FoundKey {
                        txn: main.txn,
                        account,
                        line,
                    },
                    date: main.date,
                    account_name: name_of(account).to_string(),
                    check_num: main.check_num.clone(),
                    payee: main.payee.clone(),
                    category,
                    tags: tag_names.join(", "),
                    memo,
                    cleared: main.cleared,
                    amount,
                },
                tag_names,
            };
        out.push(row(
            None,
            category,
            tag_names,
            main.txn_memo.clone(),
            main.amount,
        ));
        if within_splits && others.len() > 1 {
            for o in others {
                let mut t = o.tags.clone();
                t.sort_by_key(|t| t.to_lowercase());
                out.push(row(
                    Some(o.line_no),
                    target_text(o),
                    t,
                    o.memo.clone(),
                    // Register sign: a line has the same sign as the entry.
                    o.amount.checked_neg().unwrap_or(o.amount),
                ));
            }
        }
    }
    out
}

/// A Find row with its value read once.
struct Prepared {
    field: FindField,
    comparison: FindComparison,
    /// Lower-cased text.
    text: String,
    amount: Option<Money>,
    date: Option<Date>,
}

impl Prepared {
    fn new(c: &FindCondition) -> Result<Prepared> {
        let value = c.value.trim();
        let ordered = !matches!(
            c.comparison,
            FindComparison::Contains | FindComparison::StartsWith | FindComparison::EndsWith
        );
        let mut p = Prepared {
            field: c.field,
            comparison: c.comparison,
            text: value.to_lowercase(),
            amount: None,
            date: None,
        };
        match c.field {
            FindField::Amount => {
                p.text = p.text.replace([',', '$'], "");
                if ordered {
                    p.amount = Some(p.text.parse().map_err(|_| {
                        Error::Invalid(format!("Amount: “{value}” is not an amount"))
                    })?);
                }
            }
            FindField::Date if ordered => {
                p.date = Some(
                    value
                        .parse()
                        .map_err(|_| Error::Invalid(format!("Date: “{value}” is not a date")))?,
                );
            }
            _ => {}
        }
        Ok(p)
    }

    fn matches(&self, c: &Candidate, format: DateFormat) -> bool {
        let r = &c.row;
        match self.field {
            FindField::Amount => match self.amount {
                Some(v) => self.ordered(r.amount.cmp(&v)),
                None => self.text_match(&r.amount.to_string()),
            },
            FindField::Date => match self.date {
                Some(v) => self.ordered(r.date.cmp(&v)),
                None => self.text_match(&show_date(r.date, format)),
            },
            FindField::CheckNum => {
                match (r.check_num.trim().parse::<u64>(), self.text.parse::<u64>()) {
                    (Ok(a), Ok(b)) if !self.is_text_comparison() => self.ordered(a.cmp(&b)),
                    _ => self.text_match(&r.check_num),
                }
            }
            FindField::Category => self.text_match(&r.category),
            FindField::Cleared => self.text_match(cleared_name(r.cleared)),
            FindField::Memo => self.text_match(&r.memo),
            FindField::Payee => self.text_match(&r.payee),
            // Any one tag will do; no tag is blank.
            FindField::Tag if c.tag_names.is_empty() => self.text_match(""),
            FindField::Tag => c.tag_names.iter().any(|t| self.text_match(t)),
        }
    }

    fn is_text_comparison(&self) -> bool {
        matches!(
            self.comparison,
            FindComparison::Contains | FindComparison::StartsWith | FindComparison::EndsWith
        )
    }

    fn text_match(&self, field: &str) -> bool {
        let f = field.to_lowercase();
        let v = self.text.as_str();
        match self.comparison {
            FindComparison::Contains => f.contains(v),
            FindComparison::StartsWith => f.starts_with(v),
            FindComparison::EndsWith => f.ends_with(v),
            _ => self.ordered(f.as_str().cmp(v)),
        }
    }

    /// `ord` is the field compared with the value.
    fn ordered(&self, ord: Ordering) -> bool {
        match self.comparison {
            FindComparison::Exactly => ord == Ordering::Equal,
            FindComparison::Greater => ord == Ordering::Greater,
            FindComparison::GreaterOrEqual => ord != Ordering::Less,
            FindComparison::Less => ord == Ordering::Less,
            FindComparison::LessOrEqual => ord != Ordering::Greater,
            // Text comparisons on a number or date use its text.
            FindComparison::Contains | FindComparison::StartsWith | FindComparison::EndsWith => {
                false
            }
        }
    }
}

/// Cleared status as Find compares it and Replace takes it.
pub fn cleared_name(c: Cleared) -> &'static str {
    match c {
        Cleared::Unmarked => "Unmarked",
        Cleared::Cleared => "Cleared",
        Cleared::Reconciled => "Reconciled",
    }
}

/// A date as the register shows it (SET-030).
fn show_date(d: Date, format: DateFormat) -> String {
    let n = d.naive();
    match format {
        DateFormat::Mdy => n.format("%m/%d/%Y").to_string(),
        DateFormat::Dmy => n.format("%d/%m/%Y").to_string(),
        DateFormat::Ymd => n.format("%Y-%m-%d").to_string(),
    }
}

fn sort(rows: &mut [FoundRow], by: FindSort, descending: bool) {
    let text = |s: &str| s.to_lowercase();
    rows.sort_by(|a, b| {
        let primary = match by {
            FindSort::Date => Ordering::Equal,
            FindSort::Account => text(&a.account_name).cmp(&text(&b.account_name)),
            FindSort::CheckNum => match (a.check_num.parse::<u64>(), b.check_num.parse::<u64>()) {
                (Ok(x), Ok(y)) => x.cmp(&y),
                _ => text(&a.check_num).cmp(&text(&b.check_num)),
            },
            FindSort::Payee => text(&a.payee).cmp(&text(&b.payee)),
            FindSort::Category => text(&a.category).cmp(&text(&b.category)),
            FindSort::Tag => text(&a.tags).cmp(&text(&b.tags)),
            FindSort::Memo => text(&a.memo).cmp(&text(&b.memo)),
            FindSort::Cleared => cleared_name(a.cleared).cmp(cleared_name(b.cleared)),
            FindSort::Amount => a.amount.cmp(&b.amount),
        };
        let primary = if descending {
            primary.reverse()
        } else {
            primary
        };
        // Then by date (the sort's direction when sorting by date, else
        // newest first), and a split's lines after its row.
        let date = a.date.cmp(&b.date).then(a.key.txn.cmp(&b.key.txn));
        let date = if by == FindSort::Date && !descending {
            date
        } else {
            date.reverse()
        };
        primary
            .then(date)
            .then(a.key.account.cmp(&b.key.account))
            .then(a.key.line.cmp(&b.key.line))
    });
}

// ---------------------------------------------------------------------------
// Replace
// ---------------------------------------------------------------------------

/// What to ask before Replace All, or `None`: a new date for the checked
/// transactions, or some of them reconciled (TXN-050). One question for
/// the batch.
pub fn replace_confirmation(
    conn: &Connection,
    rows: &[FoundKey],
    spec: &ReplaceSpec,
) -> Result<Option<String>> {
    let ids: BTreeSet<TxnId> = rows.iter().map(|k| k.txn).collect();
    let mut reconciled = 0;
    for id in &ids {
        if let Some(t) = repo::find(conn, *id)?
            && t.is_reconciled()
        {
            reconciled += 1;
        }
    }
    let mut parts = Vec::new();
    if spec.field == FindField::Date {
        parts.push(format!(
            "This moves {} to {}.",
            count(ids.len(), "transaction"),
            spec.with.trim()
        ));
    }
    if reconciled > 0 {
        parts.push(format!(
            "{} of the checked {} reconciled.",
            reconciled,
            if ids.len() == 1 {
                "transaction is"
            } else {
                "transactions are"
            }
        ));
    }
    Ok((!parts.is_empty()).then(|| format!("{} Replace anyway?", parts.join(" "))))
}

fn count(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

/// The value Replace sets, read and looked up once.
enum With {
    Amount(Money),
    Date(Date),
    Cleared(Cleared),
    Category(String),
    CheckNum(String),
    Memo(String),
    Payee(Option<crate::categories::PayeeId>),
    Tag(TagId),
}

/// Why a transaction was left alone.
enum Skip {
    Split,
    Transfer,
}

/// Replace All (REG-170): set `spec` on each checked row, one audited
/// update per transaction. Needs `confirmed` when
/// [`replace_confirmation`] asks. Amount changes only a simple
/// transaction (one category line); Category changes every category
/// line of a split row, or the one split line; neither touches a
/// transfer. Tags are added; a new tag, payee, or category is created.
pub fn replace(
    tx: &Tx<'_>,
    rows: &[FoundKey],
    spec: &ReplaceSpec,
    confirmed: bool,
) -> Result<ReplaceResult> {
    replace_tracked(tx, rows, spec, confirmed).map(|(r, _)| r)
}

/// [`replace`], also returning the tags, payees, and categories it
/// created, in the order made, for undo.
pub fn replace_tracked(
    tx: &Tx<'_>,
    rows: &[FoundKey],
    spec: &ReplaceSpec,
    confirmed: bool,
) -> Result<(ReplaceResult, Vec<Created>)> {
    let before = list_ids(tx.conn())?;
    let result = replace_rows(tx, rows, spec, confirmed)?;
    let created = list_ids(tx.conn())?
        .into_iter()
        .filter(|c| !before.contains(c))
        .collect();
    Ok((result, created))
}

/// Every tag, payee, and category, by kind then ID (creation order).
fn list_ids(conn: &Connection) -> Result<Vec<Created>> {
    let mut out: Vec<Created> = categories::list(conn)?
        .into_iter()
        .map(|c| Created::Category(c.id))
        .collect();
    out.extend(
        payees::list(conn)?
            .into_iter()
            .map(|p| Created::Payee(p.id)),
    );
    out.extend(tags::list(conn)?.into_iter().map(|t| Created::Tag(t.id)));
    out.sort();
    Ok(out)
}

fn replace_rows(
    tx: &Tx<'_>,
    rows: &[FoundKey],
    spec: &ReplaceSpec,
    confirmed: bool,
) -> Result<ReplaceResult> {
    if rows.is_empty() {
        return Err(Error::Invalid("no rows are checked".into()));
    }
    if !confirmed && let Some(ask) = replace_confirmation(tx.conn(), rows, spec)? {
        return Err(Error::ConfirmationRequired(ask));
    }
    let with = read_with(tx, spec)?;
    let mut by_txn: BTreeMap<TxnId, BTreeSet<FoundKey>> = BTreeMap::new();
    for k in rows {
        by_txn.entry(k.txn).or_default().insert(*k);
    }
    let mut result = ReplaceResult::default();
    for (id, keys) in by_txn {
        let Some(txn) = repo::find(tx.conn(), id)? else {
            result.skipped_gone += 1;
            continue;
        };
        if txn.status == TxnStatus::Void || repo::is_investment_txn(tx.conn(), id)? {
            result.skipped_gone += 1;
            continue;
        }
        let closed = txn.accounts().try_fold(false, |closed, a| {
            Ok::<_, Error>(closed || accounts::get(tx.conn(), a)?.status == AccountStatus::Closed)
        })?;
        if closed {
            result.skipped_closed += 1;
            continue;
        }
        let mut input = TxnInput {
            date: txn.date,
            payee: txn.payee,
            check_num: txn.check_num.clone(),
            memo: txn.memo.clone(),
            notes: txn.notes.clone(),
            postings: txn
                .postings
                .iter()
                .map(|p| PostingInput {
                    target: p.target,
                    amount: p.amount,
                    memo: p.memo.clone(),
                    cleared: p.cleared,
                    tags: p.tags.clone(),
                    security: p.security,
                })
                .collect(),
        };
        let lines: Vec<i64> = txn.postings.iter().map(|p| p.line_no).collect();
        let original = input.clone();
        let mut skip = None;
        let mut applied = false;
        for k in &keys {
            match apply(tx, &mut input, &lines, k, &with)? {
                Ok(()) => applied = true,
                Err(s) => skip = Some(s),
            }
        }
        if !applied {
            match skip {
                Some(Skip::Split) => result.skipped_split += 1,
                Some(Skip::Transfer) => result.skipped_transfer += 1,
                None => result.unchanged += 1,
            }
            continue;
        }
        for p in &mut input.postings {
            p.tags.sort();
            p.tags.dedup();
        }
        if input == original {
            result.unchanged += 1;
            continue;
        }
        super::service::update(tx, id, &input, true)?;
        result.changed += 1;
    }
    Ok(result)
}

fn read_with(tx: &Tx<'_>, spec: &ReplaceSpec) -> Result<With> {
    let text = spec.with.trim();
    let required = |what: &str| {
        if text.is_empty() {
            Err(Error::Invalid(format!("type the {what} to replace with")))
        } else {
            Ok(())
        }
    };
    Ok(match spec.field {
        FindField::Amount => {
            required("amount")?;
            With::Amount(crate::calc::eval_amount(text)?)
        }
        FindField::Date => {
            required("date")?;
            With::Date(
                text.parse()
                    .map_err(|_| Error::Invalid(format!("“{text}” is not a date")))?,
            )
        }
        FindField::Cleared => With::Cleared(match text.to_lowercase().as_str() {
            "unmarked" | "" => Cleared::Unmarked,
            "cleared" => Cleared::Cleared,
            _ => {
                return Err(Error::Invalid(
                    "Cleared status can be set to Unmarked or Cleared; only reconciling \
                     marks a transaction Reconciled."
                        .into(),
                ));
            }
        }),
        FindField::Category => {
            required("category")?;
            With::Category(text.to_string())
        }
        FindField::CheckNum => With::CheckNum(text.to_string()),
        FindField::Memo => With::Memo(text.to_string()),
        FindField::Payee => With::Payee(if text.is_empty() {
            None
        } else {
            let name = settings::tidy_name(tx.conn(), text)?;
            Some(payees::find_or_insert(tx, &name)?.id)
        }),
        FindField::Tag => {
            required("tag")?;
            let existing = tags::list(tx.conn())?
                .into_iter()
                .find(|t| t.fields.name.trim().eq_ignore_ascii_case(text));
            With::Tag(match existing {
                Some(t) => t.id,
                None => tags::insert(tx, &TagFields::new(text))?.id,
            })
        }
    })
}

/// Set the value on one row of `input`. `lines` are the postings' line
/// numbers, in order.
fn apply(
    tx: &Tx<'_>,
    input: &mut TxnInput,
    lines: &[i64],
    key: &FoundKey,
    with: &With,
) -> Result<std::result::Result<(), Skip>> {
    let main = input
        .postings
        .iter()
        .position(|p| p.target == Target::Account(key.account) && p.security.is_none())
        .ok_or_else(|| {
            Error::Invalid(format!(
                "transaction {} has no line in account {}",
                key.txn.0, key.account.0
            ))
        })?;
    let line =
        match key.line {
            None => None,
            Some(n) => Some(lines.iter().position(|l| *l == n).ok_or_else(|| {
                Error::Invalid(format!("transaction {} has no line {n}", key.txn.0))
            })?),
        };
    match with {
        With::Date(d) => input.date = *d,
        With::Payee(p) => input.payee = *p,
        With::CheckNum(n) => input.check_num = n.clone(),
        With::Memo(m) => match line {
            Some(i) => input.postings[i].memo = m.clone(),
            None => input.memo = m.clone(),
        },
        With::Cleared(c) => input.postings[main].cleared = *c,
        With::Tag(t) => input.postings[line.unwrap_or(main)].tags.push(*t),
        With::Amount(v) => {
            if line.is_some() {
                return Ok(Err(Skip::Split));
            }
            let others: Vec<usize> = (0..input.postings.len()).filter(|i| *i != main).collect();
            match others.as_slice() {
                [o] if matches!(input.postings[*o].target, Target::Category(_)) => {
                    input.postings[main].amount = *v;
                    input.postings[*o].amount =
                        v.checked_neg().ok_or(Error::Overflow("replace amount"))?;
                }
                [_] | [] => return Ok(Err(Skip::Transfer)),
                _ => return Ok(Err(Skip::Split)),
            }
        }
        With::Category(path) => {
            let targets: Vec<usize> = match line {
                Some(i) => vec![i],
                None => (0..input.postings.len()).filter(|i| *i != main).collect(),
            };
            let targets: Vec<usize> = targets
                .into_iter()
                .filter(|i| matches!(input.postings[*i].target, Target::Category(_)))
                .collect();
            let Some(first) = targets.first() else {
                return Ok(Err(Skip::Transfer));
            };
            // A new category is income when the line brings money in.
            let kind = if input.postings[*first].amount.is_negative() {
                CategoryKind::Income
            } else {
                CategoryKind::Expense
            };
            let id = categories::create_path(tx, path, kind)?.id;
            for i in targets {
                input.postings[i].target = Target::Category(id);
            }
        }
    }
    Ok(Ok(()))
}
