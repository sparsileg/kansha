//! Reports (RPT-010 … RPT-150) and the dashboard (DSH-010 … DSH-030).
//!
//! Every report is built here as one shape, [`Report`]: named columns and
//! a tree of rows. Groups carry their totals, so a collapsed group shows
//! them and an expanded one lists its children and closes with a total
//! line. The table view, CSV export, and printing all read that shape;
//! nothing is computed in the frontend.
//!
//! [`ReportSettings`] is what the Customize dialog edits and what a saved
//! report stores (RPT-020). Signs follow the reports, not the ledger:
//! income and money coming in are positive, expenses and money going out
//! negative (a category or transfer amount is minus its posting).

mod capital_gains;
mod chart;
mod csv;
mod dashboard;
mod facts;
mod income_expense;
mod investing;
mod itemized;
mod net_worth;
mod range;
mod security;
mod tax;
mod tree;

pub use chart::{Chart, Series, SeriesStyle, Tick, XUnit};
pub use csv::to_csv;
pub use dashboard::{Dashboard, Warning, WarningKind, dashboard, net_worth};
pub use range::{period_label, periods, resolve};
pub use security::{
    ChartSpan, SecurityChartKind, SecurityTxn, security_chart, security_transactions, span_dates,
};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;
use crate::categories::{CategoryId, PayeeId, TagId};
use crate::date::Date;
use crate::error::Result;
use crate::ledger::TxnId;
use crate::securities::{AssetClass, SecurityId};
use crate::text_enum::text_enum;

/// Row ID of a saved report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct SavedReportId(pub i64);

text_enum! {
    /// The reports Kansha builds.
    pub enum ReportKind {
        /// Realized gains by lot (RPT-150).
        CapitalGains = "capital_gains",
        /// Balances by account over time, with a graph (RPT-110).
        NetWorth = "net_worth",
        /// Transactions by category (RPT-100, RPT-200).
        ItemizedCategories = "itemized_categories",
        /// Transactions by payee.
        ItemizedPayees = "itemized_payees",
        /// Category totals, optionally by period (RPT-100).
        IncomeExpense = "income_expense",
        /// Payee totals, optionally by period.
        IncomeExpensePayee = "income_expense_payee",
        /// Value, money in, income, gain, IRR, and time-weighted return
        /// by account and security for a period (POS-030).
        Performance = "performance",
        /// Investment income by account and security (RPT-160).
        InvestmentIncome = "investment_income",
        /// Positions as of a date (RPT-170).
        Holdings = "holdings",
        /// Market value by asset class, with a graph (RPT-180).
        AssetAllocation = "asset_allocation",
        /// Tax-line totals and their transactions (CAT-050).
        TaxSchedule = "tax_schedule",
        /// Tax-related categories and their transactions (RPT-140).
        TaxSummary = "tax_summary",
    }
}

text_enum! {
    /// Date range presets (RPT-040).
    pub enum DatePreset {
        AllDates = "all_dates",
        MonthToDate = "month_to_date",
        QuarterToDate = "quarter_to_date",
        YearToDate = "year_to_date",
        ThisMonth = "this_month",
        LastMonth = "last_month",
        ThisQuarter = "this_quarter",
        LastQuarter = "last_quarter",
        ThisYear = "this_year",
        LastYear = "last_year",
        Last30Days = "last_30_days",
        Last12Months = "last_12_months",
        Custom = "custom",
    }
}

text_enum! {
    /// Column periods (income and expense) and balance dates (net worth).
    pub enum Interval {
        None = "none",
        Week = "week",
        TwoWeeks = "two_weeks",
        HalfMonth = "half_month",
        Month = "month",
        Quarter = "quarter",
        HalfYear = "half_year",
        Year = "year",
    }
}

text_enum! {
    /// Capital gains grouping ("Subtotal by").
    pub enum Subtotal {
        None = "none",
        Term = "term",
        Month = "month",
        Quarter = "quarter",
        Year = "year",
        Account = "account",
        Security = "security",
    }
}

text_enum! {
    /// Order of transactions inside a group.
    pub enum DetailSort {
        /// Date, then account.
        Date = "date",
        /// Account, then date.
        AccountDate = "account_date",
        Amount = "amount",
        /// Check number (numbers in numeric order, then text, then none).
        Num = "num",
    }
}

/// A date range: a preset, or `Custom` with its own dates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DateRange {
    pub preset: DatePreset,
    /// `Custom` only; `None` means from the first transaction.
    pub from: Option<Date>,
    /// `Custom` only; `None` means today.
    pub to: Option<Date>,
}

/// A range resolved against today: `from` is `None` when no transaction
/// exists yet under "all dates".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ResolvedRange {
    pub from: Option<Date>,
    pub to: Date,
}

/// Everything the Customize dialog sets (RPT-020). Filters are `None` for
/// "all"; a list names exactly what is included. Options a report does
/// not use are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ReportSettings {
    pub kind: ReportKind,
    pub title: String,
    pub range: DateRange,
    /// Capital gains.
    #[serde(default = "default_subtotal")]
    pub subtotal: Subtotal,
    /// Net worth, income and expense.
    #[serde(default = "default_interval")]
    pub interval: Interval,
    /// Itemized and tax reports.
    #[serde(default = "default_sort")]
    pub sort: DetailSort,
    /// Reverse `sort`.
    #[serde(default)]
    pub sort_desc: bool,
    /// Column IDs not shown.
    #[serde(default)]
    pub hidden_columns: Vec<String>,
    /// Show cents; otherwise amounts round to whole dollars.
    #[serde(default = "yes")]
    pub cents: bool,
    /// Groups and totals only, no transactions.
    #[serde(default)]
    pub totals_only: bool,
    /// Net worth: list accounts whose balances are all zero.
    #[serde(default)]
    pub show_zero: bool,
    /// Itemized reports: include transfers between accounts.
    #[serde(default = "yes")]
    pub transfers: bool,
    /// `None` is every account, except Capital Gains, where it is every
    /// taxable investment account.
    #[serde(default)]
    pub accounts: Option<Vec<AccountId>>,
    #[serde(default)]
    pub categories: Option<Vec<CategoryId>>,
    #[serde(default)]
    pub payees: Option<Vec<PayeeId>>,
    #[serde(default)]
    pub securities: Option<Vec<SecurityId>>,
    #[serde(default)]
    pub tags: Option<Vec<TagId>>,
}

const fn yes() -> bool {
    true
}
const fn default_subtotal() -> Subtotal {
    Subtotal::Term
}
const fn default_interval() -> Interval {
    Interval::None
}
const fn default_sort() -> DetailSort {
    DetailSort::Date
}

impl ReportSettings {
    /// A report's standard settings (Quicken's defaults, where it has
    /// them).
    pub fn defaults(kind: ReportKind) -> ReportSettings {
        use ReportKind as K;
        let (title, preset) = match kind {
            K::CapitalGains => ("Capital Gains", DatePreset::LastYear),
            K::NetWorth => ("Net Worth", DatePreset::YearToDate),
            K::ItemizedCategories => ("Itemized Categories", DatePreset::YearToDate),
            K::ItemizedPayees => ("Itemized Payees", DatePreset::YearToDate),
            K::IncomeExpense => ("Income/Expense by Category", DatePreset::YearToDate),
            K::IncomeExpensePayee => ("Income/Expense by Payee", DatePreset::YearToDate),
            K::Performance => ("Investment Performance", DatePreset::YearToDate),
            K::InvestmentIncome => ("Investment Income", DatePreset::YearToDate),
            K::Holdings => ("Holdings", DatePreset::YearToDate),
            K::AssetAllocation => ("Asset Allocation", DatePreset::YearToDate),
            K::TaxSchedule => ("Tax Schedule", DatePreset::LastYear),
            K::TaxSummary => ("Tax Summary", DatePreset::YearToDate),
        };
        ReportSettings {
            kind,
            title: title.into(),
            range: DateRange {
                preset,
                from: None,
                to: None,
            },
            subtotal: Subtotal::Term,
            interval: if kind == K::NetWorth {
                Interval::Month
            } else {
                Interval::None
            },
            sort: if kind == K::ItemizedPayees {
                DetailSort::AccountDate
            } else {
                DetailSort::Date
            },
            sort_desc: false,
            hidden_columns: Vec::new(),
            cents: true,
            totals_only: false,
            show_zero: false,
            transfers: true,
            accounts: None,
            categories: None,
            payees: None,
            securities: None,
            tags: None,
        }
    }

    fn includes<T: PartialEq>(filter: &Option<Vec<T>>, value: &T) -> bool {
        filter.as_ref().is_none_or(|ids| ids.contains(value))
    }

    fn account_ok(&self, id: AccountId) -> bool {
        Self::includes(&self.accounts, &id)
    }

    /// Replace `from` with `to` in one filter list (a merge), or drop it
    /// when `to` is `None` (a delete). A list left empty becomes no
    /// filter. Returns whether the settings changed.
    pub fn replace_filter_id(&mut self, list: FilterList, from: i64, to: Option<i64>) -> bool {
        match list {
            FilterList::Accounts => {
                replace_id(&mut self.accounts, AccountId(from), to.map(AccountId))
            }
            FilterList::Categories => {
                replace_id(&mut self.categories, CategoryId(from), to.map(CategoryId))
            }
            FilterList::Payees => replace_id(&mut self.payees, PayeeId(from), to.map(PayeeId)),
            FilterList::Securities => {
                replace_id(&mut self.securities, SecurityId(from), to.map(SecurityId))
            }
            FilterList::Tags => replace_id(&mut self.tags, TagId(from), to.map(TagId)),
        }
    }
}

/// A [`ReportSettings`] filter list that names records by ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterList {
    Accounts,
    Categories,
    Payees,
    Securities,
    Tags,
}

/// `from` becomes `to` in place (dropped if `to` is already listed), or
/// is dropped when `to` is `None`.
fn replace_id<T: Copy + PartialEq>(filter: &mut Option<Vec<T>>, from: T, to: Option<T>) -> bool {
    let Some(ids) = filter.as_mut() else {
        return false;
    };
    let Some(at) = ids.iter().position(|id| *id == from) else {
        return false;
    };
    match to {
        Some(to) if !ids.contains(&to) => ids[at] = to,
        _ => {
            ids.remove(at);
        }
    }
    if ids.is_empty() {
        *filter = None;
    }
    true
}

/// A saved report (RPT-020).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SavedReport {
    pub id: SavedReportId,
    pub name: String,
    pub settings: ReportSettings,
}

text_enum! {
    /// How a column's cells read and align.
    pub enum ColumnKind {
        Text = "text",
        Date = "date",
        Money = "money",
        Quantity = "quantity",
        /// A percent with two decimals, e.g. "12.34".
        Percent = "percent",
    }
}

/// One column. The row label column comes first and is not listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Column {
    pub id: String,
    pub label: String,
    pub kind: ColumnKind,
    /// Period columns: the dates they cover (a balance column has only
    /// `to`), for drilling into one period.
    pub from: Option<Date>,
    pub to: Option<Date>,
}

text_enum! {
    /// What a row is.
    pub enum RowKind {
        /// Top level: INCOME, EXPENSES, ASSETS, a tax form.
        Section = "section",
        /// A category, payee, account group, or other grouping.
        Group = "group",
        /// One transaction, lot, or account.
        Detail = "detail",
        /// The closing total of the whole report.
        Total = "total",
    }
}

/// Where a figure comes from (RPT-030).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Drill {
    /// One transaction, shown in this account's register.
    Txn {
        account: AccountId,
        txn: TxnId,
        date: Date,
    },
    /// An account's register up to the column's date.
    Account { account: AccountId },
    /// A category's transactions for the column's period (an Itemized
    /// Categories report).
    Category { category: CategoryId },
    /// The holdings in one asset class (a Holdings report).
    AssetClass { asset_class: AssetClass },
    /// A payee's transactions for the column's period (an Itemized Payees
    /// report); `None` is transactions with no payee.
    Payee { payee: Option<PayeeId> },
}

/// One row. `cells` line up with the report's columns; empty text is an
/// empty cell. A group's money cells hold its totals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Row {
    pub kind: RowKind,
    pub label: String,
    pub cells: Vec<String>,
    pub drill: Option<Drill>,
    pub children: Vec<Row>,
}

/// A finished report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Report {
    pub kind: ReportKind,
    pub title: String,
    /// A line under the dates, e.g. "(Includes unrealized gains)".
    pub note: String,
    pub from: Option<Date>,
    pub to: Date,
    /// Balances as of `to` rather than a period.
    pub as_of: bool,
    /// Money shows cents; otherwise it is already rounded to dollars.
    pub cents: bool,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    pub chart: Option<Chart>,
}

/// Build a report.
pub fn run(conn: &Connection, settings: &ReportSettings, today: Date) -> Result<Report> {
    let range = resolve(conn, &settings.range, today)?;
    let mut report = match settings.kind {
        ReportKind::CapitalGains => capital_gains::build(conn, settings, range)?,
        ReportKind::NetWorth => net_worth::build(conn, settings, range)?,
        ReportKind::ItemizedCategories => {
            itemized::build(conn, settings, range, itemized::By::Category)?
        }
        ReportKind::ItemizedPayees => itemized::build(conn, settings, range, itemized::By::Payee)?,
        ReportKind::TaxSummary => itemized::build(conn, settings, range, itemized::By::TaxSummary)?,
        ReportKind::IncomeExpense => {
            income_expense::build(conn, settings, range, income_expense::By::Category)?
        }
        ReportKind::IncomeExpensePayee => {
            income_expense::build(conn, settings, range, income_expense::By::Payee)?
        }
        ReportKind::TaxSchedule => tax::build(conn, settings, range)?,
        ReportKind::Performance => investing::performance(conn, settings, range)?,
        ReportKind::InvestmentIncome => investing::income(conn, settings, range)?,
        ReportKind::Holdings => investing::holdings(conn, settings, range)?,
        ReportKind::AssetAllocation => investing::allocation(conn, settings, range)?,
    };
    tree::hide_columns(&mut report, &settings.hidden_columns);
    if !settings.cents {
        tree::round_to_dollars(&mut report)?;
    }
    Ok(report)
}

/// Columns a report kind can show, for the Customize dialog's list (net
/// worth and income/expense columns depend on the dates and are not
/// listed).
pub fn columns(kind: ReportKind) -> Vec<Column> {
    match kind {
        ReportKind::CapitalGains => capital_gains::columns(),
        ReportKind::ItemizedCategories => itemized::columns(itemized::By::Category),
        ReportKind::ItemizedPayees => itemized::columns(itemized::By::Payee),
        ReportKind::TaxSummary => itemized::columns(itemized::By::TaxSummary),
        ReportKind::TaxSchedule => tax::columns(),
        ReportKind::Performance => investing::performance_columns(),
        ReportKind::InvestmentIncome => investing::income_columns(),
        ReportKind::Holdings => investing::holdings_columns(),
        ReportKind::AssetAllocation => investing::allocation_columns(),
        ReportKind::NetWorth | ReportKind::IncomeExpense | ReportKind::IncomeExpensePayee => {
            Vec::new()
        }
    }
}
