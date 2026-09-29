//! The household dashboard (DSH-010 … DSH-030): net worth and its parts,
//! this month's income and spending, a year of net worth, what is due,
//! and what needs attention.

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::Serialize;

use super::chart::{self, Chart, SeriesStyle};
use super::facts::{self, Lookups, Section, Want};
use super::net_worth::shown_balance;
use super::range::{day_before, periods};
use super::{DatePreset, DateRange, ReportKind, ReportSettings, ResolvedRange};
use crate::accounts::{AccountId, AccountStatus};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::Money;
use crate::persistence::{accounts, reports as repo};
use crate::schedule::{self, OccurrenceView};
use crate::settings::{self, BackupStatus};
use crate::text_enum::text_enum;
use crate::{integrity, invest};

/// Uncleared transactions older than this many days are flagged.
pub const UNCLEARED_DAYS: i64 = 60;

text_enum! {
    /// What a warning is about (DSH-030).
    pub enum WarningKind {
        StalePrice = "stale_price",
        MissingPrice = "missing_price",
        Unreconciled = "unreconciled",
        Integrity = "integrity",
        /// The backup folder is missing, no backup was ever made, or the
        /// last backup's snapshot had integrity problems (BAK-030).
        Backup = "backup",
    }
}

/// Something that needs attention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Warning {
    pub kind: WarningKind,
    pub message: String,
    /// The account to open, if any.
    pub account: Option<AccountId>,
}

/// The dashboard's figures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Dashboard {
    pub today: Date,
    /// Assets minus liabilities today (DSH-010).
    pub net_worth: Money,
    /// Checking, savings, cash, and money market accounts.
    pub cash: Money,
    /// Investment accounts at market value.
    pub investments: Money,
    /// Every other asset (houses, vehicles, ...).
    pub other_assets: Money,
    /// Owed on credit cards, loans, and other liabilities (positive).
    pub liabilities: Money,
    /// This month so far.
    pub month_from: Date,
    pub income: Money,
    /// Spent (positive).
    pub expenses: Money,
    /// Income minus spending.
    pub net: Money,
    /// Net worth at each of the last twelve month ends, today last.
    pub trend: Chart,
    /// Overdue and upcoming scheduled transactions (DSH-020).
    pub upcoming: Vec<OccurrenceView>,
    pub upcoming_days: i64,
    pub warnings: Vec<Warning>,
    /// Last backup and last full verification (DSH-030, BAK-080).
    pub backup: BackupStatus,
}

fn add(a: Money, b: Money) -> Result<Money> {
    a.checked_add(b).ok_or(Error::Overflow("dashboard"))
}

pub fn dashboard(conn: &Connection, today: Date, upcoming_days: i64) -> Result<Dashboard> {
    let all = accounts::list(conn)?;
    let mut cash = Money::ZERO;
    let mut investments = Money::ZERO;
    let mut other_assets = Money::ZERO;
    let mut liabilities = Money::ZERO;
    for a in &all {
        let v = shown_balance(conn, a, today)?;
        let t = a.fields.account_type;
        let slot = if t.is_liability() {
            &mut liabilities
        } else if t.is_investment() {
            &mut investments
        } else if t.is_cash_bearing() {
            &mut cash
        } else {
            &mut other_assets
        };
        *slot = add(*slot, v)?;
    }
    let assets = add(add(cash, investments)?, other_assets)?;
    let net_worth = assets
        .checked_sub(liabilities)
        .ok_or(Error::Overflow("dashboard"))?;

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
    let expenses = spent.checked_neg().ok_or(Error::Overflow("dashboard"))?;
    let net = add(income, spent)?;

    // Net worth at each month end for a year.
    let year_ago = Date::from_naive(
        month_from
            .naive()
            .checked_sub_months(chrono::Months::new(11))
            .ok_or(Error::Overflow("date"))?,
    );
    let mut dates = vec![day_before(year_ago)?];
    dates.extend(
        periods(year_ago, today, super::Interval::Month)?
            .into_iter()
            .map(|(_, end)| end),
    );
    let mut values = Vec::with_capacity(dates.len());
    for d in &dates {
        let mut total = Money::ZERO;
        for a in &all {
            let v = shown_balance(conn, a, *d)?;
            total = if a.fields.account_type.is_liability() {
                total.checked_sub(v).ok_or(Error::Overflow("dashboard"))?
            } else {
                add(total, v)?
            };
        }
        values.push(total);
    }
    let trend = chart::build(dates, vec![("Net Worth".into(), SeriesStyle::Line, values)])?;

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

    Ok(Dashboard {
        today,
        net_worth,
        cash,
        investments,
        other_assets,
        liabilities,
        month_from,
        income,
        expenses,
        net,
        trend,
        upcoming,
        upcoming_days: days,
        warnings: warnings(conn, today, &lk)?,
        backup: settings::backup_status(conn)?,
    })
}

fn warnings(conn: &Connection, today: Date, lk: &Lookups) -> Result<Vec<Warning>> {
    let mut out = Vec::new();

    // Prices, once per security.
    let mut prices: BTreeMap<String, Warning> = BTreeMap::new();
    for a in &lk.accounts {
        if !a.fields.account_type.is_investment() || a.status != AccountStatus::Open {
            continue;
        }
        for p in invest::holdings(conn, a.id, today, None)?.positions {
            if p.shares.is_zero() {
                continue;
            }
            let label = p.ticker.clone().unwrap_or_else(|| p.name.clone());
            let w = if p.market_value.is_none() {
                Warning {
                    kind: WarningKind::MissingPrice,
                    message: format!("{label} has no price."),
                    account: Some(a.id),
                }
            } else if p.stale {
                let when = p
                    .price_date
                    .map_or_else(String::new, |d| format!(" (last {d})"));
                Warning {
                    kind: WarningKind::StalePrice,
                    message: format!("{label} price is out of date{when}."),
                    account: Some(a.id),
                }
            } else {
                continue;
            };
            prices.entry(label).or_insert(w);
        }
    }
    out.extend(prices.into_values());

    // Accounts with old uncleared transactions.
    let before = schedule::add_days(today, -UNCLEARED_DAYS).unwrap_or(today);
    for id in repo::accounts_with_old_uncleared(conn, before)? {
        let Some(a) = lk.account(id) else { continue };
        if a.status != AccountStatus::Open {
            continue;
        }
        out.push(Warning {
            kind: WarningKind::Unreconciled,
            message: format!(
                "{} has uncleared transactions more than {UNCLEARED_DAYS} days old.",
                a.fields.name
            ),
            account: Some(id),
        });
    }

    let report = integrity::check(conn)?;
    if !report.is_clean() {
        out.push(Warning {
            kind: WarningKind::Integrity,
            message: format!(
                "The integrity check found {} problem{}.",
                report.issues.len(),
                if report.issues.len() == 1 { "" } else { "s" }
            ),
            account: None,
        });
    }

    let b = settings::backup_status(conn)?;
    if b.folder_missing {
        out.push(Warning {
            kind: WarningKind::Backup,
            message: "The backup folder is missing, so backups go to Downloads. Choose a folder in Settings.".into(),
            account: None,
        });
    }
    if b.last_at.is_none() {
        out.push(Warning {
            kind: WarningKind::Backup,
            message: "No backup has been made yet.".into(),
            account: None,
        });
    } else if b.last_issues > 0 {
        out.push(Warning {
            kind: WarningKind::Backup,
            message: format!(
                "The last backup was made with {} integrity problem{}.",
                b.last_issues,
                if b.last_issues == 1 { "" } else { "s" }
            ),
            account: None,
        });
    }
    Ok(out)
}
