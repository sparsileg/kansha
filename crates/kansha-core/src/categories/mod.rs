//! Categories, payees, and tags (CAT, PAY, TAG).

use serde::{Deserialize, Serialize};

use crate::date::Timestamp;
use crate::money::Money;
use crate::text_enum::text_enum;

/// Row ID of a category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct CategoryId(pub i64);

/// Row ID of a payee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct PayeeId(pub i64);

/// Row ID of a tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct TagId(pub i64);

text_enum! {
    /// Category kind (CAT-010). `Equity` is system-only (opening balances).
    pub enum CategoryKind {
        Income = "income",
        Expense = "expense",
        Equity = "equity",
    }
}

text_enum! {
    /// Built-in categories seeded by migration 0001 (CAT-060, RCN-040).
    pub enum SystemCategory {
        Dividends = "dividends",
        Interest = "interest",
        CapGainDistShort = "cg_dist_short",
        CapGainDistLong = "cg_dist_long",
        RealizedGain = "realized_gain",
        InvestmentIncome = "investment_income",
        InvestmentFees = "investment_fees",
        InvestmentExpense = "investment_expense",
        TaxWithheld = "tax_withheld",
        BalanceAdjustment = "balance_adjustment",
        OpeningBalance = "opening_balance",
    }
}

/// Editable attributes of a category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CategoryFields {
    pub parent: Option<CategoryId>,
    pub kind: CategoryKind,
    pub name: String,
    /// CAT-040 flags.
    pub tax_related: bool,
    pub tithable: bool,
    pub giving: bool,
    /// CAT-030: hide instead of delete.
    pub hidden: bool,
}

impl CategoryFields {
    pub fn new(name: impl Into<String>, kind: CategoryKind) -> CategoryFields {
        CategoryFields {
            parent: None,
            kind,
            name: name.into(),
            tax_related: false,
            tithable: false,
            giving: false,
            hidden: false,
        }
    }

    pub fn under(mut self, parent: CategoryId) -> CategoryFields {
        self.parent = Some(parent);
        self
    }
}

/// A stored category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Category {
    pub id: CategoryId,
    #[serde(flatten)]
    pub fields: CategoryFields,
    /// Set for built-in categories, which can't be renamed, moved, or
    /// deleted.
    pub system: Option<SystemCategory>,
    pub created_at: Timestamp,
}

/// Editable attributes of a payee, including memorized defaults (PAY-020).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PayeeFields {
    pub name: String,
    pub default_category: Option<CategoryId>,
    pub default_tag: Option<TagId>,
    pub default_memo: String,
    pub default_amount: Option<Money>,
    pub hidden: bool,
}

impl PayeeFields {
    pub fn new(name: impl Into<String>) -> PayeeFields {
        PayeeFields {
            name: name.into(),
            default_category: None,
            default_tag: None,
            default_memo: String::new(),
            default_amount: None,
            hidden: false,
        }
    }
}

/// A stored payee.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Payee {
    pub id: PayeeId,
    #[serde(flatten)]
    pub fields: PayeeFields,
    pub created_at: Timestamp,
}

/// Editable attributes of a tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TagFields {
    pub name: String,
    pub hidden: bool,
}

impl TagFields {
    pub fn new(name: impl Into<String>) -> TagFields {
        TagFields {
            name: name.into(),
            hidden: false,
        }
    }
}

/// A stored tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Tag {
    pub id: TagId,
    #[serde(flatten)]
    pub fields: TagFields,
    pub created_at: Timestamp,
}

/// What a merge moved from the source to the target (CAT-020, PAY-030,
/// TAG-020). Recorded as the `after` value of the merge's audit entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Merged {
    /// ID of the surviving category, payee, or tag.
    pub into: i64,
    pub postings: usize,
    pub txns: usize,
    pub schedules: usize,
    pub schedule_lines: usize,
    pub payee_defaults: usize,
    pub subcategories: usize,
}
