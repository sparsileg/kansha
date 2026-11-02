//! Security master and prices (SEC-010 … SEC-040, PRC-010 … PRC-050).
//!
//! Domain types. Storage is `persistence::securities`; the price list
//! import is [`preview_prices`] / [`commit_prices`] (PRC-030); price
//! download is [`download`] (PRC-040).

pub mod download;
mod import;

pub use import::{PriceImportPreview, PriceImportRow, commit_prices, preview_prices};

use serde::{Deserialize, Serialize};

use crate::accounts::LotMethod;
use crate::date::{Date, Timestamp};
use crate::error::{Error, Result};
use crate::money::{Money, Price, Rate, allocate};
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
        UsSmallMid = "us_small_mid",
        IntlEquity = "intl_equity",
        Emerging = "emerging",
        BondShort = "bond_short",
        BondIntermediate = "bond_intermediate",
        BondLong = "bond_long",
        BondIntl = "bond_intl",
        BondTips = "bond_tips",
        BondMuni = "bond_muni",
        Cash = "cash",
        RealEstate = "real_estate",
        Commodity = "commodity",
        Other = "other",
    }
}

impl AssetClass {
    /// Its place in lists: the enum's order.
    pub fn order(self) -> usize {
        Self::ALL
            .iter()
            .position(|c| *c == self)
            .unwrap_or(usize::MAX)
    }

    /// How reports name it.
    pub const fn label(self) -> &'static str {
        match self {
            AssetClass::UsEquity => "US equity",
            AssetClass::UsSmallMid => "US small/mid",
            AssetClass::IntlEquity => "Intl equity",
            AssetClass::Emerging => "Emerging mkts",
            AssetClass::BondShort => "Short-term bonds",
            AssetClass::BondIntermediate => "Intermed bonds",
            AssetClass::BondLong => "Long-term bonds",
            AssetClass::BondIntl => "Intl bonds",
            AssetClass::BondTips => "TIPS",
            AssetClass::BondMuni => "Muni bonds",
            AssetClass::Cash => "Cash",
            AssetClass::RealEstate => "Real estate",
            AssetClass::Commodity => "Commodities",
            AssetClass::Other => "Other",
        }
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

/// One class's part of a security split across classes (SEC-050).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ClassShare {
    pub asset_class: AssetClass,
    /// Percent, at most two decimals (`"60"`, `"12.5"`).
    pub percent: Rate,
}

/// Percent ×10^6 of a whole security.
const WHOLE: i64 = 100_000_000;

/// Check a class mix and put it in order: largest share first, ties in
/// class order. Empty is no mix. Otherwise two or more distinct classes,
/// each above 0 with at most two decimals, totalling exactly 100.
pub fn normalize_mix(mix: &[ClassShare]) -> Result<Vec<ClassShare>> {
    if mix.is_empty() {
        return Ok(Vec::new());
    }
    if mix.len() < 2 {
        return Err(Error::Invalid(
            "a class mix needs two or more classes".into(),
        ));
    }
    let mut total: i64 = 0;
    for (i, c) in mix.iter().enumerate() {
        let p = c.percent.raw();
        if p <= 0 || p >= WHOLE {
            return Err(Error::Invalid(format!(
                "{}: a share is more than 0% and less than 100%",
                c.asset_class.label()
            )));
        }
        if p % 10_000 != 0 {
            return Err(Error::Invalid(format!(
                "{}: a share has at most two decimals",
                c.asset_class.label()
            )));
        }
        if mix[..i].iter().any(|o| o.asset_class == c.asset_class) {
            return Err(Error::Invalid(format!(
                "{} is listed twice",
                c.asset_class.label()
            )));
        }
        total += p;
    }
    if total != WHOLE {
        return Err(Error::Invalid(format!(
            "the shares total {}%, not 100%",
            Rate::from_raw(total)
        )));
    }
    let mut out = mix.to_vec();
    out.sort_by(|a, b| {
        b.percent
            .cmp(&a.percent)
            .then(a.asset_class.order().cmp(&b.asset_class.order()))
    });
    Ok(out)
}

/// Split `value` across a mix (SEC-050): each class's percent of it, in
/// cents, adding up to `value` exactly (largest remainder, §19). With no
/// mix, all of it in `class`.
pub fn split_value(
    value: Money,
    class: AssetClass,
    mix: &[ClassShare],
) -> Result<Vec<(AssetClass, Money)>> {
    if mix.is_empty() {
        return Ok(vec![(class, value)]);
    }
    let weights: Vec<i64> = mix.iter().map(|c| c.percent.raw()).collect();
    let negative = value.is_negative();
    let whole = if negative {
        value.checked_neg().ok_or(Error::Overflow("class split"))?
    } else {
        value
    };
    let parts = allocate(whole.cents(), &weights)?;
    Ok(mix
        .iter()
        .zip(parts)
        .map(|(c, p)| {
            let m = Money::from_cents(p);
            (c.asset_class, if negative { -m } else { m })
        })
        .collect())
}

/// Editable attributes of a security (SEC-010 … SEC-030).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SecurityFields {
    pub name: String,
    /// Unique when present; CDs and some bonds have none.
    pub ticker: Option<String>,
    pub security_type: SecurityType,
    /// The class it counts in; with a mix, the class of its largest share.
    pub asset_class: AssetClass,
    /// Split across several classes (SEC-050): empty for none, else two or
    /// more distinct classes whose percents total 100.
    pub class_mix: Vec<ClassShare>,
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
            SecurityType::Cd => AssetClass::BondShort,
            SecurityType::Bond => AssetClass::BondIntermediate,
            SecurityType::MoneyMarket => AssetClass::Cash,
            _ => AssetClass::UsEquity,
        };
        SecurityFields {
            name: name.into(),
            ticker: None,
            security_type,
            asset_class,
            class_mix: Vec::new(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn share(c: AssetClass, p: &str) -> ClassShare {
        ClassShare {
            asset_class: c,
            percent: p.parse().unwrap(),
        }
    }

    fn m(s: &str) -> Money {
        s.parse().unwrap()
    }

    #[test]
    fn bond_and_cd_default_to_the_new_bond_classes() {
        let f = SecurityFields::new("CD", SecurityType::Cd);
        assert_eq!(f.asset_class, AssetClass::BondShort);
        let f = SecurityFields::new("Bond", SecurityType::Bond);
        assert_eq!(f.asset_class, AssetClass::BondIntermediate);
        assert_eq!(AssetClass::BondIntl.label(), "Intl bonds");
        assert_eq!(AssetClass::BondShort.order(), 4);
        assert_eq!(AssetClass::BondMuni.label(), "Muni bonds");
    }

    #[test]
    fn a_mix_totals_100_with_distinct_classes_and_two_decimals() {
        use AssetClass::*;
        assert!(normalize_mix(&[]).unwrap().is_empty());
        let ok =
            normalize_mix(&[share(BondIntermediate, "39.5"), share(UsEquity, "60.5")]).unwrap();
        assert_eq!(ok[0].asset_class, UsEquity, "largest share first");
        // Equal shares keep class order.
        let tie = normalize_mix(&[share(Cash, "50"), share(UsEquity, "50")]).unwrap();
        assert_eq!(tie[0].asset_class, UsEquity);
        let bad = |mix: &[ClassShare]| normalize_mix(mix).unwrap_err().to_string();
        assert!(bad(&[share(UsEquity, "100")]).contains("two or more"));
        assert!(bad(&[share(UsEquity, "60"), share(Cash, "30")]).contains("total 90%"));
        assert!(bad(&[share(UsEquity, "60"), share(UsEquity, "40")]).contains("twice"));
        assert!(bad(&[share(UsEquity, "60.005"), share(Cash, "39.995")]).contains("two decimals"));
        assert!(bad(&[share(UsEquity, "100"), share(Cash, "0")]).contains("more than 0%"));
        assert!(bad(&[share(UsEquity, "110"), share(Cash, "-10")]).contains("more than 0%"));
    }

    #[test]
    fn split_value_adds_up_to_the_cent() {
        use AssetClass::*;
        assert_eq!(
            split_value(m("333.35"), Other, &[]).unwrap(),
            vec![(Other, m("333.35"))]
        );
        let mix = [share(UsEquity, "60"), share(BondIntermediate, "40")];
        assert_eq!(
            split_value(m("333.35"), UsEquity, &mix).unwrap(),
            vec![(UsEquity, m("200.01")), (BondIntermediate, m("133.34"))]
        );
        // 3.333 + 3.333 + 3.334 cents: the cent left goes to the largest
        // remainder.
        let thirds = [
            share(UsEquity, "33.33"),
            share(IntlEquity, "33.33"),
            share(BondShort, "33.34"),
        ];
        assert_eq!(
            split_value(m("0.10"), UsEquity, &thirds).unwrap(),
            vec![
                (UsEquity, m("0.03")),
                (IntlEquity, m("0.03")),
                (BondShort, m("0.04"))
            ]
        );
        // A negative value splits the same way, signs kept.
        let neg = split_value(m("-333.35"), UsEquity, &mix).unwrap();
        assert_eq!(
            neg,
            vec![(UsEquity, m("-200.01")), (BondIntermediate, m("-133.34"))]
        );
    }
}
