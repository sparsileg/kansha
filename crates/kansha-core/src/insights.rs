//! Insights (INS-010 … INS-040): named views of dashboard cards, shown as
//! tabs. Each is a name and an ordered list of card IDs; one card can be
//! on several insights. The card catalog and the cards' contents belong
//! to the UI, so the IDs are only checked for shape here. Stored by
//! [`crate::persistence::insights`].

use serde::{Deserialize, Serialize};

/// Row ID of an insight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct InsightId(pub i64);

/// One insight, a tab of cards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Insight {
    pub id: InsightId,
    pub name: String,
    /// Card IDs, in the order they show.
    pub cards: Vec<String>,
}
