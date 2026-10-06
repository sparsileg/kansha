//! Reports (RPT-010 … RPT-150) and the figures the Insights cards show
//! (CARD-010 … CARD-030).
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
mod cards;
mod chart;
mod compare;
mod csv;
mod facts;
mod income_expense;
mod investing;
mod itemized;
mod net_worth;
mod range;
mod security;
mod tax;
mod tree;

pub use cards::{
    Attention, AttentionCheck, CardData, CheckKind, ExpenseCard, ExpenseRow, Finding, Notice,
    Session, account_bar_net_worth, age, attention, auto_expenses, card_data, net_worth,
    net_worth_trend,
};
pub use chart::{Chart, Series, SeriesStyle, Tick, XUnit};
pub use csv::to_csv;
pub use range::{compare_before, compare_window, period_choices, period_label, periods, resolve};
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

/// Row ID of a saved report folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct ReportFolderId(pub i64);

impl ReportFolderId {
    /// The permanent "Unfiled" folder, where new saved reports go.
    pub const UNFILED: Self = Self(1);
}

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
        /// Spending in a period against the average of the periods
        /// before it, by category (RPT-210).
        CompareCategory = "compare_category",
        /// The same by payee (RPT-210).
        ComparePayee = "compare_payee",
    }
}

impl ReportKind {
    /// Shown in the compact layout (RPT-145, RPT-205), with totals on
    /// the group headings by default (RPT-020).
    pub fn compact(self) -> bool {
        matches!(
            self,
            Self::TaxSchedule | Self::TaxSummary | Self::ItemizedCategories | Self::ItemizedPayees
        )
    }
}

text_enum! {
    /// Date range presets (RPT-040). This month, quarter, and year are
    /// no longer offered but still read in saved reports and used by
    /// the Insights cards.
    pub enum DatePreset {
        AllDates = "all_dates",
        /// One calendar month, quarter, or year, chosen from a list
        /// (Tax Schedule, RPT-040). `from` is its first day.
        Monthly = "monthly",
        Quarterly = "quarterly",
        Yearly = "yearly",
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
        /// From the first day of this week (Settings, SET-030) to today.
        WeekToDate = "week_to_date",
        LastWeek = "last_week",
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
    /// Tax Summary grouping ("Subtotal by", RPT-140). Apart from
    /// [`Subtotal`] so neither report offers the other's choices.
    pub enum TaxGroup {
        /// INCOME, EXPENSES, TRANSFERS, then the category tree.
        Category = "category",
        /// Form, then line, in Tax Schedule order.
        TaxLine = "tax_line",
        Account = "account",
        Payee = "payee",
        Tag = "tag",
        Month = "month",
        Quarter = "quarter",
        Year = "year",
        None = "none",
    }
}

text_enum! {
    /// What the comparison reports average (RPT-210): the periods
    /// ending on the report's last day.
    pub enum CompareTo {
        Weeks4 = "weeks_4",
        Weeks8 = "weeks_8",
        Weeks12 = "weeks_12",
        Months3 = "months_3",
        Months6 = "months_6",
        Months12 = "months_12",
        Quarters4 = "quarters_4",
        Quarters8 = "quarters_8",
        Quarters12 = "quarters_12",
        Years1 = "years_1",
        Years3 = "years_3",
        Years5 = "years_5",
    }
}

impl CompareTo {
    const WEEKS: &[CompareTo] = &[Self::Weeks4, Self::Weeks8, Self::Weeks12];
    const MONTHS: &[CompareTo] = &[Self::Months3, Self::Months6, Self::Months12];
    const QUARTERS: &[CompareTo] = &[Self::Quarters4, Self::Quarters8, Self::Quarters12];
    const YEARS: &[CompareTo] = &[Self::Years1, Self::Years3, Self::Years5];

    /// The choices a date range offers: weeks for a week, months for a
    /// month, and so on; every choice for a custom range.
    pub fn choices(preset: DatePreset) -> &'static [CompareTo] {
        use DatePreset as P;
        match preset {
            P::WeekToDate | P::LastWeek => Self::WEEKS,
            P::MonthToDate | P::ThisMonth | P::LastMonth => Self::MONTHS,
            P::QuarterToDate | P::ThisQuarter | P::LastQuarter => Self::QUARTERS,
            P::YearToDate | P::ThisYear | P::LastYear => Self::YEARS,
            _ => Self::ALL,
        }
    }

    /// `self` when the range offers it, else the range's first choice.
    pub fn for_range(self, preset: DatePreset) -> CompareTo {
        let choices = Self::choices(preset);
        if choices.contains(&self) {
            self
        } else {
            choices[0]
        }
    }

    /// The "Compare to" text, as the dropdown shows it.
    pub fn label(self) -> String {
        let unit = match self {
            Self::Weeks4 | Self::Weeks8 | Self::Weeks12 => "weeks",
            Self::Months3 | Self::Months6 | Self::Months12 => "months",
            Self::Quarters4 | Self::Quarters8 | Self::Quarters12 => "quarters",
            Self::Years1 => return "Last year".into(),
            Self::Years3 | Self::Years5 => "years",
        };
        format!("Last {} {unit}", self.count())
    }

    /// How many periods are averaged.
    pub fn count(self) -> u32 {
        match self {
            Self::Years1 => 1,
            Self::Months3 | Self::Years3 => 3,
            Self::Weeks4 | Self::Quarters4 => 4,
            Self::Years5 => 5,
            Self::Months6 => 6,
            Self::Weeks8 | Self::Quarters8 => 8,
            Self::Weeks12 | Self::Months12 | Self::Quarters12 => 12,
        }
    }
}

text_enum! {
    /// The comparison reports' "Subtotal by" (RPT-210). Category is
    /// offered by the payee report only, Payee by the category report
    /// only.
    pub enum CompareGroup {
        None = "none",
        Category = "category",
        Payee = "payee",
        Tag = "tag",
        Account = "account",
        TaxLine = "tax_line",
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
    /// `Custom`: `None` means from the first transaction. Monthly,
    /// Quarterly, Yearly: a day in the period; `None` means today's.
    pub from: Option<Date>,
    /// `Custom` only; `None` means today.
    pub to: Option<Date>,
}

/// One period offered for Monthly, Quarterly, or Yearly: its dates and
/// its name ("Oct 2026", "Q4 2026", "2026").
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PeriodChoice {
    pub from: Date,
    pub to: Date,
    pub label: String,
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
    /// Tax Summary.
    #[serde(default = "default_tax_group")]
    pub tax_group: TaxGroup,
    /// Comparison reports: what the date range is set against.
    #[serde(default = "default_compare")]
    pub compare: CompareTo,
    /// Comparison reports.
    #[serde(default = "default_compare_group")]
    pub compare_group: CompareGroup,
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
    /// A group's totals on its heading line, with no closing total line;
    /// `None` is the report's default ([`Self::totals_on_heading`]).
    #[serde(default)]
    pub totals_on_heading: Option<bool>,
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
const fn default_tax_group() -> TaxGroup {
    TaxGroup::Category
}
const fn default_compare() -> CompareTo {
    CompareTo::Months3
}
const fn default_compare_group() -> CompareGroup {
    CompareGroup::None
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
            K::TaxSummary => ("Tax Summary", DatePreset::LastYear),
            K::CompareCategory => (
                "Current Spending vs. Average by Category",
                DatePreset::MonthToDate,
            ),
            K::ComparePayee => (
                "Current Spending vs. Average by Payee",
                DatePreset::MonthToDate,
            ),
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
            tax_group: TaxGroup::Category,
            compare: CompareTo::Months3,
            compare_group: CompareGroup::None,
            interval: if kind == K::NetWorth {
                Interval::Month
            } else {
                Interval::None
            },
            sort: if matches!(kind, K::ItemizedPayees | K::TaxSummary) {
                DetailSort::AccountDate
            } else {
                DetailSort::Date
            },
            sort_desc: false,
            // The S column (split marker) shows by default only in the
            // tax reports, which hide Tag instead.
            hidden_columns: match kind {
                K::ItemizedCategories | K::ItemizedPayees => vec!["split".into()],
                K::TaxSchedule | K::TaxSummary => vec!["tag".into()],
                _ => Vec::new(),
            },
            cents: true,
            totals_only: false,
            totals_on_heading: None,
            show_zero: false,
            transfers: true,
            accounts: None,
            categories: None,
            payees: None,
            securities: None,
            tags: None,
        }
    }

    /// Whether a group's totals go on its heading line: as set, else on
    /// for the compact reports (RPT-020).
    pub fn totals_on_heading(&self) -> bool {
        self.totals_on_heading.unwrap_or(self.kind.compact())
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
    pub folder: ReportFolderId,
}

/// A folder of saved reports (RPT-020). One level; each report is in
/// exactly one folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ReportFolder {
    pub id: ReportFolderId,
    pub name: String,
    /// "Unfiled": never renamed or deleted.
    pub permanent: bool,
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
    /// A group's totals are on its heading line; no closing total line.
    pub totals_on_heading: bool,
    /// Quicken's narrow layout (Tax Schedule and Tax Summary, RPT-145): the first column
    /// sits under the group headings, text is cut to fit the page width,
    /// and top-level rows are shaded.
    pub compact: bool,
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
        ReportKind::CompareCategory => {
            compare::build(conn, settings, range, today, compare::By::Category)?
        }
        ReportKind::ComparePayee => {
            compare::build(conn, settings, range, today, compare::By::Payee)?
        }
    };
    tree::hide_columns(&mut report, &settings.hidden_columns);
    report.totals_on_heading = settings.totals_on_heading();
    report.compact = settings.kind.compact();
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
        ReportKind::NetWorth
        | ReportKind::IncomeExpense
        | ReportKind::IncomeExpensePayee
        | ReportKind::CompareCategory
        | ReportKind::ComparePayee => Vec::new(),
    }
}
