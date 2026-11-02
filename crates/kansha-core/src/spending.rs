//! Spending cards (CARD-060): named Insights cards, each listing its
//! chosen spending categories over its chosen accounts and tags, in its
//! chosen columns (CARD-070). An insight shows one by the card ID
//! `spending:<id>`. Stored by
//! [`crate::persistence::spending`]; figured by
//! [`crate::reports::spending_card`].

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;
use crate::categories::{CategoryId, TagId};
use crate::text_enum::text_enum;

/// Row ID of a spending card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct SpendingCardId(pub i64);

impl SpendingCardId {
    /// The card ID an insight stores for this card.
    pub fn card_id(self) -> String {
        format!("{CARD_PREFIX}{}", self.0)
    }
}

/// Start of a spending card's card ID on an insight.
pub const CARD_PREFIX: &str = "spending:";

text_enum! {
    /// A column a spending card can show, in display order.
    pub enum SpendingColumn {
        LastYear = "last_year",
        Year = "year",
        LastMonth = "last_month",
        Month = "month",
        MonthlyAvg = "monthly_avg",
        ThreeYearAvg = "three_year_avg",
    }
}

/// One spending card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SpendingCard {
    pub id: SpendingCardId,
    /// Unique, any case; the card's heading.
    pub name: String,
    /// The accounts counted; `None` = every open account (never
    /// customized).
    pub accounts: Option<Vec<AccountId>>,
    /// The spending categories listed, each on its own. Empty with
    /// `tags` chosen: every spending category with a tagged line.
    pub categories: Vec<CategoryId>,
    /// A line counts when it has any of these; empty counts every line.
    pub tags: Vec<TagId>,
    /// The columns shown, in display order; never empty.
    pub columns: Vec<SpendingColumn>,
}
