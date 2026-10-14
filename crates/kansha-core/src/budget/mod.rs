//! Budgets (BUD-010 … BUD-080): named budgets of income and expense
//! categories, one amount per category and month of each year a budget
//! holds. Stored by [`crate::persistence::budgets`]; actuals come from
//! the ledger (BUD-030) and every figure the budget view shows is worked
//! out here (BUD-040 … BUD-070).

mod period;
mod service;
mod view;

pub use period::{GraphAt, GraphPeriod};
pub use service::{
    copy, copy_month, create, create_year, delete, rename, set, set_categories, set_period,
};
pub use view::{
    AnnualGroup, AnnualRow, AnnualView, Bar, BudgetFigures, GraphGroup, GraphRow, GraphView,
    MonthColumn, Standing, Summary, annual, graph,
};

use serde::{Deserialize, Serialize};

use crate::categories::CategoryId;
use crate::money::Money;
use crate::text_enum::text_enum;

/// Row ID of a budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct BudgetId(pub i64);

/// A budget: its name, categories, and years (BUD-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Budget {
    pub id: BudgetId,
    /// Unique, any case.
    pub name: String,
    /// Income and expense categories at any level, by ID.
    pub categories: Vec<CategoryId>,
    /// The years it holds, oldest first.
    pub years: Vec<i32>,
}

/// One month's budget for one category. Zero is not stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BudgetCell {
    pub year: i32,
    pub month: u32,
    pub category: CategoryId,
    /// Signed: a refund category may have a negative budget.
    pub amount: Money,
}

/// A budget with every amount: what its audit entries record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetRecord {
    pub budget: Budget,
    pub cells: Vec<BudgetCell>,
}

text_enum! {
    /// Which months a month's budget is copied to (BUD-070), in its year.
    pub enum CopyScope {
        Future = "future",
        Past = "past",
        All = "all",
    }
}
