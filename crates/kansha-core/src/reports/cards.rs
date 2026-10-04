//! What the Insights cards show (CARD-010 … CARD-050): net worth and its
//! parts, this month's income and spending, net worth over time, what is
//! due, and what needs attention.

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::Serialize;

use super::chart::{self, Chart, SeriesStyle};
use super::facts::{self, Lookups, Section, Want};
use super::net_worth::shown_balances;
use super::range::{day_before, periods};
use super::{DatePreset, DateRange, ReportKind, ReportSettings, ResolvedRange};
use crate::accounts::{AccountId, AccountStatus, AccountType};
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
    /// What a warning is about (CARD-030).
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

/// The figures the Insights cards show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CardData {
    pub today: Date,
    /// Assets minus liabilities today (CARD-010).
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
    /// Overdue and upcoming scheduled transactions (CARD-020).
    pub upcoming: Vec<OccurrenceView>,
    pub upcoming_days: i64,
    pub warnings: Vec<Warning>,
    /// Last backup and last full verification (CARD-030, BAK-080).
    pub backup: BackupStatus,
}

fn add(a: Money, b: Money) -> Result<Money> {
    a.checked_add(b).ok_or(Error::Overflow("insight cards"))
}

/// Net worth's parts today: cash, investments, other assets, and
/// liabilities (owed, positive), every account counted.
struct Parts {
    cash: Money,
    investments: Money,
    other_assets: Money,
    liabilities: Money,
}

impl Parts {
    fn load(conn: &Connection, today: Date) -> Result<Parts> {
        let mut p = Parts {
            cash: Money::ZERO,
            investments: Money::ZERO,
            other_assets: Money::ZERO,
            liabilities: Money::ZERO,
        };
        for a in &accounts::list(conn)? {
            let v = shown_balances(conn, a, &[today])?[0];
            let t = a.fields.account_type;
            let slot = if t.is_liability() {
                &mut p.liabilities
            } else if t.is_investment() {
                &mut p.investments
            } else if t.is_cash_bearing() {
                &mut p.cash
            } else {
                &mut p.other_assets
            };
            *slot = add(*slot, v)?;
        }
        Ok(p)
    }

    fn net_worth(&self) -> Result<Money> {
        add(add(self.cash, self.investments)?, self.other_assets)?
            .checked_sub(self.liabilities)
            .ok_or(Error::Overflow("insight cards"))
    }
}

/// Net worth today, as the Net worth card shows it (CARD-010): for the foot of
/// the account list (ACCT-240).
pub fn net_worth(conn: &Connection, today: Date) -> Result<Money> {
    Parts::load(conn, today)?.net_worth()
}

pub fn card_data(conn: &Connection, today: Date, upcoming_days: i64) -> Result<CardData> {
    let parts = Parts::load(conn, today)?;
    let net_worth = parts.net_worth()?;
    let Parts {
        cash,
        investments,
        other_assets,
        liabilities,
    } = parts;

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
        cash,
        investments,
        other_assets,
        liabilities,
        month_from,
        income,
        expenses,
        net,
        upcoming,
        upcoming_days: days,
        warnings: warnings(conn, today, &lk)?,
        backup: settings::backup_status(conn)?,
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

    // Accounts with old uncleared transactions (CARD-030): only checking,
    // savings, and credit card accounts, each by name. Others (cash,
    // investment, assets) are not reconciled against statements.
    let before = schedule::add_days(today, -UNCLEARED_DAYS).unwrap_or(today);
    for id in repo::accounts_with_old_uncleared(conn, before)? {
        let Some(a) = lk.account(id) else { continue };
        if a.status != AccountStatus::Open
            || !matches!(
                a.fields.account_type,
                AccountType::Checking | AccountType::Savings | AccountType::CreditCard
            )
        {
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
