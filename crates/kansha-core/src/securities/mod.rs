//! Security master and prices (SEC-010 … SEC-040, PRC-010 … PRC-050).
//!
//! Domain types. Storage is `persistence::securities`; price CSV import
//! is [`import_prices`].

mod import;

pub use import::{PriceImportPreview, PriceImportRow, commit_prices, preview_prices};

use serde::{Deserialize, Serialize};

use crate::accounts::LotMethod;
use crate::date::{Date, Timestamp};
use crate::money::Price;
use crate::text_enum::text_enum;

/// Row ID of a security.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct SecurityId(pub i64);

text_enum! {
    /// Kind of security (SEC-010).
    pub enum SecurityType {
        Stock = "stock",
        Etf = "etf",
        MutualFund = "mutual_fund",
        Bond = "bond",
        MoneyMarket = "money_market",
        Cd = "cd",
        Other = "other",
    }
}

text_enum! {
    /// Asset class for allocation (SEC-010, POS-020).
    pub enum AssetClass {
        UsEquity = "us_equity",
        IntlEquity = "intl_equity",
        Bond = "bond",
        Cash = "cash",
        RealEstate = "real_estate",
        Commodity = "commodity",
        Other = "other",
    }
}

text_enum! {
    /// Where a price came from (PRC-010 … PRC-040).
    pub enum PriceSource {
        Manual = "manual",
        Csv = "csv",
        Qif = "qif",
        Download = "download",
    }
}

/// Editable attributes of a security (SEC-010 … SEC-030).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SecurityFields {
    pub name: String,
    /// Unique when present; CDs and some bonds have none.
    pub ticker: Option<String>,
    pub security_type: SecurityType,
    pub asset_class: AssetClass,
    /// Nine characters (SEC-020).
    pub cusip: Option<String>,
    /// Overrides the account's default lot selection (SEC-030, LOT-100).
    pub default_lot_method: Option<LotMethod>,
    /// Hidden securities stay in history but leave pick lists (SEC-040).
    pub hidden: bool,
    pub notes: String,
}

impl SecurityFields {
    pub fn new(name: impl Into<String>, security_type: SecurityType) -> SecurityFields {
        let asset_class = match security_type {
            SecurityType::Bond | SecurityType::Cd => AssetClass::Bond,
            SecurityType::MoneyMarket => AssetClass::Cash,
            _ => AssetClass::UsEquity,
        };
        SecurityFields {
            name: name.into(),
            ticker: None,
            security_type,
            asset_class,
            cusip: None,
            default_lot_method: None,
            hidden: false,
            notes: String::new(),
        }
    }
}

/// A stored security.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Security {
    pub id: SecurityId,
    #[serde(flatten)]
    pub fields: SecurityFields,
    pub created_at: Timestamp,
}

impl Security {
    /// Ticker if it has one, otherwise the name: how lists show it.
    pub fn label(&self) -> &str {
        self.fields.ticker.as_deref().unwrap_or(&self.fields.name)
    }
}

/// One closing price (PRC-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PricePoint {
    pub security: SecurityId,
    pub date: Date,
    pub price: Price,
    pub source: PriceSource,
}

/// Money market funds without a stored price are worth $1.00 a share
/// (INV-050).
pub const MONEY_MARKET_PRICE: Price = Price::from_raw(1_000_000);

/// Prices older than this many days are stale unless the caller says
/// otherwise (PRC-050, SET-040).
pub const DEFAULT_STALE_DAYS: i64 = 7;
