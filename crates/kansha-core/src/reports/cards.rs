//! What the Insights cards show (CARD-010 … CARD-060): net worth and its
//! parts, this month's income and spending, net worth over time, what is
//! due, what needs attention, and spending cards.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use super::chart::{self, Chart, SeriesStyle};
use super::facts::{self, Lookups, Section, Want};
use super::net_worth::shown_balances;
use super::range::{day_before, month_end, periods};
use super::{DatePreset, DateRange, ReportKind, ReportSettings, ResolvedRange};
use crate::accounts::{Account, AccountGroup, AccountId, AccountStatus, AccountType};
use crate::categories::{CategoryId, CategoryKind};
use crate::date::{Clock, Date, Timestamp};
use crate::error::{Error, Result};
use crate::integrity::IntegrityStatus;
use crate::invest;
use crate::money::{Money, mul_div};
use crate::persistence::{accounts, audit, reports as repo};
use crate::schedule::{self, OccurrenceView};
use crate::settings;
use crate::spending::SpendingCard;
use crate::text_enum::text_enum;

/// Uncleared transactions older than this many days are flagged.
pub const UNCLEARED_DAYS: i64 = 60;
/// Accounts not reconciled in this many days are flagged.
pub const RECONCILE_DAYS: i64 = 60;
/// A notice names at most this many items, then "and N more".
pub const SHOWN_ITEMS: usize = 5;

text_enum! {
    /// One check of the Needs attention card (CARD-030), in the order
    /// they are listed.
    pub enum CheckKind {
        Integrity = "integrity",
        Backup = "backup",
        Verification = "verification",
        BackupFolder = "backup_folder",
        Prices = "prices",
        Overdue = "overdue",
        Uncleared = "uncleared",
        Reconcile = "reconcile",
        Uncategorized = "uncategorized",
        InvestmentCash = "investment_cash",
    }
}

/// What the Needs attention card shows (CARD-030): a notice for each
/// problem found, and every check made, for "Show checks".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Attention {
    pub checked_at: Timestamp,
    /// `checked_at` as a local clock time: "2:32 PM".
    pub as_of: String,
    pub checks: Vec<AttentionCheck>,
    pub notices: Vec<Notice>,
}

/// One check and whether it passed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AttentionCheck {
    pub kind: CheckKind,
    /// "Security prices", "Database integrity", ...
    pub label: String,
    pub ok: bool,
    /// More about it, such as how long ago the last backup was.
    pub detail: Option<String>,
}

/// A problem a check found and what to do about it, shown as the
/// message, the items (": a, b, and 3 more."), then the remedy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Notice {
    pub kind: CheckKind,
    /// What is wrong, with no closing period.
    pub message: String,
    /// The accounts or securities it is about, at most [`SHOWN_ITEMS`].
    pub items: Vec<Finding>,
    /// How many more items there are.
    pub more: usize,
    /// What the user can do, as a sentence.
    pub remedy: String,
}

/// An item a notice names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Finding {
    pub message: String,
    /// The account to open, if any.
    pub account: Option<AccountId>,
}

impl Notice {
    fn new(kind: CheckKind, message: impl Into<String>, remedy: &str) -> Notice {
        Notice {
            kind,
            message: message.into(),
            items: Vec::new(),
            more: 0,
            remedy: remedy.into(),
        }
    }

    fn items(mut self, mut items: Vec<Finding>) -> Notice {
        self.more = items.len().saturating_sub(SHOWN_ITEMS);
        items.truncate(SHOWN_ITEMS);
        self.items = items;
        self
    }
}

/// The open book's session, for the backup checks: when the book was
/// opened, and whether the check of its last backup is still running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    pub started: Timestamp,
    pub verifying: bool,
}

/// "1 problem", "2 problems".
fn count(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

fn is(n: usize) -> &'static str {
    if n == 1 { "is" } else { "are" }
}

fn has(n: usize) -> &'static str {
    if n == 1 { "has" } else { "have" }
}

/// How long before `now` `then` was: "under a minute", "5 minutes",
/// "32 hours", "3 days", "2 weeks", "4 months".
pub fn age(now: Timestamp, then: Timestamp) -> String {
    let minutes = usize::try_from(now.seconds_since(then) / 60).unwrap_or(0);
    let hours = minutes / 60;
    let days = hours / 24;
    if minutes < 1 {
        "under a minute".into()
    } else if hours < 1 {
        count(minutes, "minute")
    } else if hours < 48 {
        count(hours, "hour")
    } else if days < 14 {
        count(days, "day")
    } else if days < 56 {
        count(days / 7, "week")
    } else {
        count(days / 30, "month")
    }
}

/// The figures the Insights cards show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CardData {
    pub today: Date,
    /// Assets minus liabilities today (CARD-010).
    pub net_worth: Money,
    /// The Net worth card's columns: two years ago and last year (at
    /// December 31), then this year (today).
    pub years: Vec<i32>,
    /// One row per account group with an account, in the account
    /// list's order.
    pub groups: Vec<GroupBalances>,
    /// Net worth at each of `years`; the last is `net_worth`.
    pub net_worths: Vec<Money>,
    /// This month so far.
    pub month_from: Date,
    pub income: Money,
    /// Spent (positive).
    pub expenses: Money,
    /// Income minus spending.
    pub net: Money,
    /// Overdue and upcoming scheduled transactions (CARD-020).
    pub upcoming: Vec<OccurrenceView>,
    pub upcoming_days: i64,
}

/// One account group's row on the Net worth card (CARD-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct GroupBalances {
    pub group: AccountGroup,
    /// As the account list names it: "Banking".
    pub label: String,
    /// At each of `CardData::years`; money owed is negative.
    pub balances: Vec<Money>,
}

fn add(a: Money, b: Money) -> Result<Money> {
    a.checked_add(b).ok_or(Error::Overflow("insight cards"))
}

/// The account list's groups, in its order (ACCT-240).
const GROUPS: [(AccountGroup, &str); 7] = [
    (AccountGroup::Banking, "Banking"),
    (AccountGroup::Credit, "Credit"),
    (AccountGroup::Investments, "Investments"),
    (AccountGroup::Retirement, "Retirement"),
    (AccountGroup::Assets, "Assets"),
    (AccountGroup::Liabilities, "Liabilities"),
    (AccountGroup::Other, "Other"),
];

/// Each group's balance on each of `dates`, every account counted
/// (closed ones too) in the group it is in now; investments at market
/// value, money owed negative. Groups with no account are left out.
fn group_balances(conn: &Connection, dates: &[Date]) -> Result<Vec<GroupBalances>> {
    let mut rows: Vec<(GroupBalances, bool)> = GROUPS
        .iter()
        .map(|(group, label)| {
            (
                GroupBalances {
                    group: *group,
                    label: (*label).to_string(),
                    balances: vec![Money::ZERO; dates.len()],
                },
                false,
            )
        })
        .collect();
    for a in &accounts::list(conn)? {
        let Some((row, used)) = rows.iter_mut().find(|(r, _)| r.group == a.fields.group) else {
            continue;
        };
        *used = true;
        let owed = a.fields.account_type.is_liability();
        for (sum, v) in row.balances.iter_mut().zip(shown_balances(conn, a, dates)?) {
            let v = if owed {
                v.checked_neg().ok_or(Error::Overflow("insight cards"))?
            } else {
                v
            };
            *sum = add(*sum, v)?;
        }
    }
    Ok(rows
        .into_iter()
        .filter(|(_, used)| *used)
        .map(|(r, _)| r)
        .collect())
}

/// Net worth at each of `dates`: the groups' sum.
fn net_worths(groups: &[GroupBalances], dates: usize) -> Result<Vec<Money>> {
    (0..dates)
        .map(|i| {
            groups
                .iter()
                .try_fold(Money::ZERO, |acc, g| add(acc, g.balances[i]))
        })
        .collect()
}

/// Net worth today, as the Net worth card shows it (CARD-010): for the foot of
/// the account list (ACCT-240).
pub fn net_worth(conn: &Connection, today: Date) -> Result<Money> {
    let groups = group_balances(conn, &[today])?;
    Ok(net_worths(&groups, 1)?
        .first()
        .copied()
        .unwrap_or(Money::ZERO))
}

/// [`net_worth`] as the account bar shows it (ACCT-240).
pub fn account_bar_net_worth(conn: &Connection, today: Date) -> Result<Money> {
    crate::ledger::account_bar_figure(conn, net_worth(conn, today)?)
}

pub fn card_data(conn: &Connection, today: Date, upcoming_days: i64) -> Result<CardData> {
    // Net worth by group: December 31 two years ago and last year, and
    // today.
    let years = vec![today.year() - 2, today.year() - 1, today.year()];
    let dates = [
        Date::from_ymd(years[0], 12, 31)?,
        Date::from_ymd(years[1], 12, 31)?,
        today,
    ];
    let groups = group_balances(conn, &dates)?;
    let net_worths = net_worths(&groups, dates.len())?;
    let net_worth = net_worths.last().copied().unwrap_or(Money::ZERO);

    // This month's income and spending.
    let month_from = Date::from_ymd(today.year(), today.month(), 1)?;
    let lk = Lookups::load(conn)?;
    let range = ResolvedRange {
        from: Some(month_from),
        to: today,
    };
    let mut settings = ReportSettings::defaults(ReportKind::IncomeExpense);
    settings.range = DateRange {
        preset: DatePreset::MonthToDate,
        from: None,
        to: None,
    };
    settings.transfers = false;
    let lines = facts::lines(&facts::load(conn, range)?, &settings, &lk, Want::All)?;
    let mut income = Money::ZERO;
    let mut spent = Money::ZERO;
    for l in &lines {
        match l.section {
            Section::Income => income = add(income, l.amount)?,
            Section::Expenses => spent = add(spent, l.amount)?,
            Section::Transfers => {}
        }
    }
    let expenses = spent
        .checked_neg()
        .ok_or(Error::Overflow("insight cards"))?;
    let net = add(income, spent)?;

    // Due: overdue ones and the next `upcoming_days`.
    let days = upcoming_days.clamp(1, 366);
    let until = schedule::add_days(today, days).unwrap_or(today);
    let mut upcoming: Vec<OccurrenceView> = schedule::due_list(conn, today)?
        .into_iter()
        .filter(|o| o.overdue)
        .collect();
    upcoming.extend(schedule::occurrences_between(
        conn, today, until, today, None, false,
    )?);
    upcoming.sort_by_key(|o| (o.date, o.nominal, o.schedule));
    upcoming.dedup_by_key(|o| (o.schedule, o.nominal));

    Ok(CardData {
        today,
        net_worth,
        years,
        groups,
        net_worths,
        month_from,
        income,
        expenses,
        net,
        upcoming,
        upcoming_days: days,
    })
}

/// Net worth at each month end over the last `years` years (1 to 5),
/// today last, for the Net worth over time card (CARD-050). `fitted`
/// sizes the money axis to the data instead of reaching zero.
pub fn net_worth_trend(conn: &Connection, today: Date, years: i64, fitted: bool) -> Result<Chart> {
    let years = years.clamp(*settings::TREND_YEARS.start(), *settings::TREND_YEARS.end());
    let month_from = Date::from_ymd(today.year(), today.month(), 1)?;
    let months = u32::try_from(years * 12 - 1).map_err(|_| Error::Overflow("date"))?;
    let first = Date::from_naive(
        month_from
            .naive()
            .checked_sub_months(chrono::Months::new(months))
            .ok_or(Error::Overflow("date"))?,
    );
    let mut dates = vec![day_before(first)?];
    dates.extend(
        periods(first, today, super::Interval::Month)?
            .into_iter()
            .map(|(_, end)| end),
    );
    let mut values = vec![Money::ZERO; dates.len()];
    for a in &accounts::list(conn)? {
        let liability = a.fields.account_type.is_liability();
        for (total, v) in values.iter_mut().zip(shown_balances(conn, a, &dates)?) {
            *total = if liability {
                total
                    .checked_sub(v)
                    .ok_or(Error::Overflow("insight cards"))?
            } else {
                add(*total, v)?
            };
        }
    }
    let series = vec![("Net Worth".into(), SeriesStyle::Line, values)];
    if fitted {
        chart::build_fitted(dates, series)
    } else {
        chart::build(dates, series)
    }
}

/// One row of a spending card (CARD-060): spent this year, this
/// month, and per month, expenses positive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ExpenseRow {
    /// `None` on the Total row.
    pub category: Option<CategoryId>,
    /// "Car:BlueForester:Gas", or "Total".
    pub label: String,
    /// January 1 through the end of this month, scheduled included.
    pub ytd: Money,
    /// The 1st of this month through its end, scheduled included.
    pub mtd: Money,
    /// `ytd` over the months begun this year, this one included, as
    /// Quicken does.
    pub monthly_avg: Money,
    /// The part of `ytd` and `mtd` still only scheduled: pending
    /// occurrences dated this month.
    pub scheduled: Money,
}

/// What a spending card shows (CARD-060).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ExpenseCard {
    /// The chosen categories with transactions this year (scheduled
    /// this month included), by name;
    /// none when no category or no account is chosen.
    pub rows: Vec<ExpenseRow>,
    pub total: ExpenseRow,
    /// How many categories are chosen (0 when no account is), so an
    /// empty card can tell "choose some" from "no spending this year".
    pub chosen: u32,
}

/// A spending card (CARD-060): each spending category chosen on the
/// card, on its own (subcategories are not added in), counting only the
/// chosen accounts (every open one if never chosen). Past and scheduled:
/// transactions through the end of this month (later-dated ones
/// included) and this month's pending scheduled occurrences, so what
/// is due later this month counts now.
pub fn spending_card(conn: &Connection, today: Date, card: &SpendingCard) -> Result<ExpenseCard> {
    let lk = Lookups::load(conn)?;
    let accounts = card.accounts.clone().unwrap_or_else(|| {
        lk.accounts
            .iter()
            .filter(|a| a.status == AccountStatus::Open)
            .map(|a| a.id)
            .collect()
    });
    let mut chosen: Vec<CategoryId> = card
        .categories
        .iter()
        .copied()
        .filter(|c| {
            lk.category(*c)
                .is_some_and(|x| x.fields.kind == CategoryKind::Expense)
        })
        .collect();
    if accounts.is_empty() {
        chosen.clear();
    }
    let months = i64::from(today.month());
    let row = |category, label, ytd: Money, mtd, scheduled| -> Result<ExpenseRow> {
        Ok(ExpenseRow {
            category,
            label,
            ytd,
            mtd,
            monthly_avg: Money::from_cents(mul_div(ytd.cents(), 1, months)?),
            scheduled,
        })
    };

    let mut ytd: BTreeMap<CategoryId, Money> = BTreeMap::new();
    let mut mtd: BTreeMap<CategoryId, Money> = BTreeMap::new();
    let mut sched: BTreeMap<CategoryId, Money> = BTreeMap::new();
    if !chosen.is_empty() {
        let year_from = Date::from_ymd(today.year(), 1, 1)?;
        let month_from = Date::from_ymd(today.year(), today.month(), 1)?;
        let month_to = month_end(today, 0)?;
        for (c, amount) in
            schedule::scheduled_by_category(conn, month_from, month_to, today, &accounts)?
        {
            if !chosen.contains(&c) {
                continue;
            }
            // A payment line is negative; spent is shown positive.
            let spent = amount
                .checked_neg()
                .ok_or(Error::Overflow("insight cards"))?;
            for m in [&mut ytd, &mut mtd, &mut sched] {
                let v = m.entry(c).or_insert(Money::ZERO);
                *v = add(*v, spent)?;
            }
        }
        let mut rs = ReportSettings::defaults(ReportKind::IncomeExpense);
        rs.accounts = Some(accounts);
        rs.categories = Some(chosen.clone());
        rs.transfers = false;
        let range = ResolvedRange {
            from: Some(year_from),
            to: month_to,
        };
        for l in facts::lines(&facts::load(conn, range)?, &rs, &lk, Want::All)? {
            let facts::Target::Category(c) = l.target else {
                continue;
            };
            // Report sign is income positive; spent is shown positive.
            let spent = l
                .amount
                .checked_neg()
                .ok_or(Error::Overflow("insight cards"))?;
            let y = ytd.entry(c).or_insert(Money::ZERO);
            *y = add(*y, spent)?;
            if l.date >= month_from {
                let m = mtd.entry(c).or_insert(Money::ZERO);
                *m = add(*m, spent)?;
            }
        }
    }

    chosen.sort_by_cached_key(|c| lk.category_path(*c).to_lowercase());
    chosen.dedup();
    let count = u32::try_from(chosen.len()).unwrap_or(u32::MAX);
    let mut rows = Vec::new();
    let (mut ty, mut tm, mut ts) = (Money::ZERO, Money::ZERO, Money::ZERO);
    for c in chosen {
        // No transactions this year: left out (one netting to zero stays).
        let Some(&y) = ytd.get(&c) else {
            continue;
        };
        let m = mtd.get(&c).copied().unwrap_or(Money::ZERO);
        let s = sched.get(&c).copied().unwrap_or(Money::ZERO);
        ty = add(ty, y)?;
        tm = add(tm, m)?;
        ts = add(ts, s)?;
        rows.push(row(Some(c), lk.category_path(c), y, m, s)?);
    }
    Ok(ExpenseCard {
        rows,
        total: row(None, "Total".into(), ty, tm, ts)?,
        chosen: count,
    })
}

/// The Needs attention card (CARD-030): every check, and a notice for
/// each problem found. `integrity` is the last integrity check of the
/// book, kept by the caller so the card does not run it each time.
pub fn attention(
    conn: &Connection,
    clock: &dyn Clock,
    session: Session,
    integrity: IntegrityStatus,
) -> Result<Attention> {
    let (today, now) = (clock.today(), clock.now());
    let lk = Lookups::load(conn)?;
    let b = settings::backup_status(conn)?;
    let mut notices = Vec::new();

    // Database integrity.
    if integrity.issues > 0 {
        notices.push(Notice::new(
            CheckKind::Integrity,
            format!(
                "The database has {}",
                count(integrity.issues, "integrity problem")
            ),
            "Show the details and fix each one.",
        ));
    }

    // Every change backed up: a change made before the book was opened
    // has had its chance at a backup.
    let back_up = "Use File > Back Up Now.";
    match (b.last_at, audit::last_change_before(conn, session.started)?) {
        (None, Some(_)) => notices.push(Notice::new(
            CheckKind::Backup,
            "The book has never been backed up",
            back_up,
        )),
        (Some(at), Some(c)) if c > at => notices.push(Notice::new(
            CheckKind::Backup,
            format!(
                "Changes made before Kansha was started have no backup; \
                 the last backup was {} ago",
                age(now, at)
            ),
            back_up,
        )),
        _ => {}
    }
    let n = usize::try_from(b.last_issues).unwrap_or(0);
    if b.last_at.is_some() && n > 0 {
        notices.push(Notice::new(
            CheckKind::Backup,
            format!(
                "The last backup was made with {}",
                count(n, "integrity problem")
            ),
            "Fix the database's integrity problems, then use File > Back Up Now.",
        ));
    }

    // The last backup verified at startup (BAK-080); Verify backup… does
    // not count. Nothing to say while the check runs.
    if !session.verifying {
        if let Some(e) = &b.startup_error {
            let file = b
                .startup_path
                .as_deref()
                .map(|p| {
                    Path::new(p)
                        .file_name()
                        .map_or(p.into(), |n| n.to_string_lossy())
                })
                .unwrap_or_default();
            notices.push(Notice::new(
                CheckKind::Verification,
                format!("The last backup could not be verified ({file}): {e}"),
                "Use File > Back Up Now to make a new one; \
                 it is checked the next time Kansha starts.",
            ));
        } else if let Some(at) = b.last_at {
            let checked = (b.startup_path.is_some() && b.startup_path == b.last_path)
                || b.startup_checked_at.is_some_and(|c| c >= session.started);
            if at < session.started && !checked {
                notices.push(Notice::new(
                    CheckKind::Verification,
                    "The last backup has not been verified",
                    "Restart Kansha; the last backup is checked \
                     after the passphrase is typed.",
                ));
            }
        }
    }

    // Backup folder (BAK-030).
    let choose = "Choose a folder in Edit > Settings.";
    match settings::load(conn)?.backup_folder {
        Some(f) if !Path::new(&f).is_dir() => notices.push(Notice::new(
            CheckKind::BackupFolder,
            format!("The backup folder {f} is not there, so backups go to Downloads"),
            choose,
        )),
        Some(_) => {}
        None => notices.push(Notice::new(
            CheckKind::BackupFolder,
            "No backup folder is chosen, so backups go to Downloads",
            choose,
        )),
    }

    let (prices, cash) = investments(conn, today, &lk)?;
    if !prices.is_empty() {
        let n = prices.len();
        notices.push(
            Notice::new(
                CheckKind::Prices,
                format!(
                    "{} {} out of date or missing",
                    count(n, "security price"),
                    is(n)
                ),
                "Update them in Tools > Securities, or use Tools > Import Prices.",
            )
            .items(prices),
        );
    }

    // Overdue reminders (REC-130).
    let n = schedule::due_list(conn, today)?
        .iter()
        .filter(|o| o.overdue)
        .count();
    if n > 0 {
        notices.push(Notice::new(
            CheckKind::Overdue,
            format!("{} {} overdue", count(n, "reminder"), is(n)),
            "Enter or skip them in Tools > Reminders.",
        ));
    }

    let found = uncleared(conn, today, &lk)?;
    if !found.is_empty() {
        let n = found.len();
        notices.push(
            Notice::new(
                CheckKind::Uncleared,
                format!(
                    "{} {} uncleared transactions more than {UNCLEARED_DAYS} days old",
                    count(n, "account"),
                    has(n)
                ),
                "Open each account and clear or reconcile them.",
            )
            .items(found),
        );
    }

    let found = unreconciled(conn, today, &lk)?;
    if !found.is_empty() {
        let n = found.len();
        notices.push(
            Notice::new(
                CheckKind::Reconcile,
                format!(
                    "{} {} not been reconciled in {RECONCILE_DAYS} days",
                    count(n, "account"),
                    has(n)
                ),
                "Reconcile each one against its latest statement (Tools > Reconcile).",
            )
            .items(found),
        );
    }

    let mut found = Vec::new();
    let mut n = 0;
    for (id, k) in repo::uncategorized_by_account(conn)? {
        let Some(a) = lk.account(id) else { continue };
        n += usize::try_from(k).unwrap_or(0);
        found.push(Finding {
            message: format!("{} ({k})", a.fields.name),
            account: Some(id),
        });
    }
    if !found.is_empty() {
        notices.push(
            Notice::new(
                CheckKind::Uncategorized,
                format!("{} {} in Uncategorized", count(n, "transaction"), is(n)),
                "Open each account and choose a category for them.",
            )
            .items(found),
        );
    }

    if !cash.is_empty() {
        let n = cash.len();
        notices.push(
            Notice::new(
                CheckKind::InvestmentCash,
                format!(
                    "{} {} cash below zero",
                    count(n, "investment account"),
                    has(n)
                ),
                "Enter the missing deposit, sale, or transfer, \
                 or correct the entry that overdrew it.",
            )
            .items(cash),
        );
    }

    let checks = CheckKind::ALL
        .iter()
        .map(|&kind| AttentionCheck {
            kind,
            label: label(kind).into(),
            ok: !notices.iter().any(|n| n.kind == kind),
            detail: match (kind, b.last_at) {
                (CheckKind::Backup, Some(at)) => Some(format!("last backup {} ago", age(now, at))),
                _ => None,
            },
        })
        .collect();
    Ok(Attention {
        checked_at: now,
        as_of: now.time_of_day(clock.utc_offset()),
        checks,
        notices,
    })
}

fn label(kind: CheckKind) -> &'static str {
    match kind {
        CheckKind::Integrity => "Database integrity",
        CheckKind::Backup => "Changes backed up",
        CheckKind::Verification => "Last backup verified",
        CheckKind::BackupFolder => "Backup folder",
        CheckKind::Prices => "Security prices",
        CheckKind::Overdue => "Overdue reminders",
        CheckKind::Uncleared => "Old uncleared transactions",
        CheckKind::Reconcile => "Accounts reconciled",
        CheckKind::Uncategorized => "Uncategorized transactions",
        CheckKind::InvestmentCash => "Investment cash",
    }
}

/// Securities with a missing or stale price, once each, and investment
/// accounts with cash below zero: open investment accounts only.
fn investments(
    conn: &Connection,
    today: Date,
    lk: &Lookups,
) -> Result<(Vec<Finding>, Vec<Finding>)> {
    let mut prices: BTreeMap<String, Finding> = BTreeMap::new();
    let mut cash = Vec::new();
    for a in &lk.accounts {
        if !a.fields.account_type.is_investment() || a.status != AccountStatus::Open {
            continue;
        }
        let h = invest::holdings(conn, a.id, today, None)?;
        if let Some(c) = h.cash.filter(|c| c.is_negative()) {
            cash.push(Finding {
                message: format!("{} ({c})", a.fields.name),
                account: Some(a.id),
            });
        }
        for p in h.positions {
            if p.shares.is_zero() {
                continue;
            }
            let label = p.ticker.clone().unwrap_or_else(|| p.name.clone());
            let message = if p.market_value.is_none() {
                format!("{label} (no price)")
            } else if p.stale {
                match p.price_date {
                    Some(d) => format!("{label} (last {d})"),
                    None => label.clone(),
                }
            } else {
                continue;
            };
            prices.entry(label).or_insert(Finding {
                message,
                account: Some(a.id),
            });
        }
    }
    Ok((prices.into_values().collect(), cash))
}

/// Open checking, savings, and credit card accounts: only these are
/// reconciled against statements.
fn statement_account(a: &Account) -> bool {
    a.status == AccountStatus::Open
        && matches!(
            a.fields.account_type,
            AccountType::Checking | AccountType::Savings | AccountType::CreditCard
        )
}

/// Statement accounts with old uncleared transactions, each by name.
fn uncleared(conn: &Connection, today: Date, lk: &Lookups) -> Result<Vec<Finding>> {
    let before = schedule::add_days(today, -UNCLEARED_DAYS).unwrap_or(today);
    let mut found = Vec::new();
    for id in repo::accounts_with_old_uncleared(conn, before)? {
        let Some(a) = lk.account(id) else { continue };
        if statement_account(a) {
            found.push(Finding {
                message: a.fields.name.clone(),
                account: Some(id),
            });
        }
    }
    Ok(found)
}

/// Statement accounts not reconciled in [`RECONCILE_DAYS`] that have a
/// transaction dated after they last were (or any, if never), each with
/// when that was.
fn unreconciled(conn: &Connection, today: Date, lk: &Lookups) -> Result<Vec<Finding>> {
    let due = schedule::add_days(today, -RECONCILE_DAYS).unwrap_or(today);
    let mut found = Vec::new();
    for (id, last) in repo::last_reconciled(conn)? {
        let Some(a) = lk.account(id) else { continue };
        if !statement_account(a) || last.is_some_and(|d| d >= due) {
            continue;
        }
        found.push(Finding {
            message: match last {
                Some(d) => format!("{} (last {d})", a.fields.name),
                None => format!("{} (never)", a.fields.name),
            },
            account: Some(id),
        });
    }
    Ok(found)
}
