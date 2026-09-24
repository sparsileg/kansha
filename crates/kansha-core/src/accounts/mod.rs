//! Accounts: types, attributes, and lifecycle (ACCT-010 … ACCT-240).
//!
//! Domain types. Storage is `persistence::accounts`; closing, which needs
//! the ledger (ACCT-210), is `ledger::close_account`.

use serde::{Deserialize, Serialize};

use crate::date::{Date, Timestamp};
use crate::money::{Money, Rate};
use crate::text_enum::text_enum;

/// Row ID of an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct AccountId(pub i64);

text_enum! {
    /// Account types (ACCT-010, ACCT-020; D-100).
    pub enum AccountType {
        Checking = "checking",
        Savings = "savings",
        CreditCard = "credit_card",
        Cash = "cash",
        MoneyMarket = "money_market",
        Brokerage = "brokerage",
        TraditionalIra = "traditional_ira",
        RothIra = "roth_ira",
        Hsa = "hsa",
        /// 401(k)/403(b).
        Retirement401k = "retirement_401k",
        OtherAsset = "other_asset",
        OtherLiability = "other_liability",
        /// Loan or mortgage; balance tracking only in 1.0.
        Loan = "loan",
    }
}

text_enum! {
    /// Account list groups (ACCT-240).
    pub enum AccountGroup {
        Banking = "banking",
        Credit = "credit",
        Investments = "investments",
        Retirement = "retirement",
        Assets = "assets",
        Liabilities = "liabilities",
    }
}

text_enum! {
    /// Tax treatment (ACCT-030).
    pub enum TaxTreatment {
        Taxable = "taxable",
        TaxDeferred = "tax_deferred",
        TaxExempt = "tax_exempt",
    }
}

text_enum! {
    /// Open or closed (ACCT-210).
    pub enum AccountStatus {
        Open = "open",
        Closed = "closed",
    }
}

text_enum! {
    /// Where an investment account's cash lives (INV-300).
    pub enum CashMode {
        Internal = "internal",
        Linked = "linked",
    }
}

text_enum! {
    /// How money market funds are held in an investment account (INV-050, D-50).
    pub enum MmfMode {
        /// A security priced at $1.00.
        Security = "security",
        /// Part of the account's cash balance.
        Cash = "cash",
    }
}

text_enum! {
    /// Lot selection methods (LOT-100, LOT-110, D-60). The prototype
    /// engine implements `Fifo` and `Specific`; the others are stored so
    /// the schema doesn't change when they arrive.
    pub enum LotMethod {
        Fifo = "fifo",
        Specific = "specific",
        Average = "average",
        /// Highest cost first.
        Hifo = "hifo",
        /// Minimize tax: short-term losses, long-term losses, long-term
        /// gains (smallest first), short-term gains (smallest first).
        MinTax = "min_tax",
    }
}

text_enum! {
    /// Kind of an Other Asset account (ACCT-140).
    pub enum AssetSubtype {
        House = "house",
        Vehicle = "vehicle",
        Other = "other",
    }
}

impl AccountType {
    /// Investment accounts hold securities and lots.
    pub const fn is_investment(self) -> bool {
        matches!(
            self,
            AccountType::Brokerage
                | AccountType::TraditionalIra
                | AccountType::RothIra
                | AccountType::Hsa
                | AccountType::Retirement401k
        )
    }

    /// Liability accounts: their ledger balance is negative while money
    /// is owed (spec §18 posting sign).
    pub const fn is_liability(self) -> bool {
        matches!(
            self,
            AccountType::CreditCard | AccountType::OtherLiability | AccountType::Loan
        )
    }

    /// Accounts that can serve as an investment account's linked cash
    /// account (INV-300).
    pub const fn is_cash_bearing(self) -> bool {
        matches!(
            self,
            AccountType::Checking
                | AccountType::Savings
                | AccountType::Cash
                | AccountType::MoneyMarket
        )
    }

    /// Default tax treatment (ACCT-030).
    pub const fn default_tax_treatment(self) -> TaxTreatment {
        match self {
            AccountType::TraditionalIra | AccountType::Retirement401k => TaxTreatment::TaxDeferred,
            AccountType::RothIra | AccountType::Hsa => TaxTreatment::TaxExempt,
            _ => TaxTreatment::Taxable,
        }
    }

    /// Default account-list group (ACCT-240).
    pub const fn default_group(self) -> AccountGroup {
        match self {
            AccountType::Checking
            | AccountType::Savings
            | AccountType::Cash
            | AccountType::MoneyMarket => AccountGroup::Banking,
            AccountType::CreditCard => AccountGroup::Credit,
            AccountType::Brokerage => AccountGroup::Investments,
            AccountType::TraditionalIra
            | AccountType::RothIra
            | AccountType::Hsa
            | AccountType::Retirement401k => AccountGroup::Retirement,
            AccountType::OtherAsset => AccountGroup::Assets,
            AccountType::OtherLiability | AccountType::Loan => AccountGroup::Liabilities,
        }
    }
}

/// Settings only investment accounts have (ACCT-130, INV-300, D-50).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InvestmentSettings {
    pub subtype: Option<String>,
    pub cash_mode: CashMode,
    /// Required when `cash_mode` is `Linked`.
    pub linked_cash_account: Option<AccountId>,
    pub mmf_mode: MmfMode,
    pub default_lot_method: LotMethod,
}

/// Settings only Other Asset accounts have (ACCT-140).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct OtherAssetSettings {
    pub subtype: AssetSubtype,
    pub linked_liability: Option<AccountId>,
}

/// Editable attributes of an account (ACCT-100 … ACCT-160). Used to
/// create and to update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AccountFields {
    pub name: String,
    pub account_type: AccountType,
    pub group: AccountGroup,
    pub tax_treatment: TaxTreatment,
    pub description: String,
    pub institution: String,
    pub account_number: String,
    pub contact_phone: String,
    pub home_url: String,
    pub notes: String,
    pub opening_date: Option<Date>,
    pub show_in_bar: bool,
    pub show_in_list: bool,
    pub sort_order: i64,
    /// Checking, Savings, Money Market only.
    pub interest_rate: Option<Rate>,
    /// Credit Card only.
    pub credit_limit: Option<Money>,
    /// Required for investment types; absent otherwise.
    pub investment: Option<InvestmentSettings>,
    /// Required for Other Asset; absent otherwise.
    pub other_asset: Option<OtherAssetSettings>,
}

impl AccountFields {
    /// Fields with the type's defaults (group, tax treatment; investment
    /// accounts get internal cash, MMF as cash, FIFO).
    pub fn new(name: impl Into<String>, account_type: AccountType) -> AccountFields {
        AccountFields {
            name: name.into(),
            account_type,
            group: account_type.default_group(),
            tax_treatment: account_type.default_tax_treatment(),
            description: String::new(),
            institution: String::new(),
            account_number: String::new(),
            contact_phone: String::new(),
            home_url: String::new(),
            notes: String::new(),
            opening_date: None,
            show_in_bar: true,
            show_in_list: true,
            sort_order: 0,
            interest_rate: None,
            credit_limit: None,
            investment: account_type.is_investment().then_some(InvestmentSettings {
                subtype: None,
                cash_mode: CashMode::Internal,
                linked_cash_account: None,
                mmf_mode: MmfMode::Cash,
                default_lot_method: LotMethod::Fifo,
            }),
            other_asset: (account_type == AccountType::OtherAsset).then_some(OtherAssetSettings {
                subtype: AssetSubtype::Other,
                linked_liability: None,
            }),
        }
    }
}

/// A stored account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Account {
    pub id: AccountId,
    #[serde(flatten)]
    pub fields: AccountFields,
    pub status: AccountStatus,
    pub closed_date: Option<Date>,
    pub created_at: Timestamp,
}

/// Account number masked to its last four characters (ACCT-150).
pub fn masked_account_number(number: &str) -> String {
    let chars: Vec<char> = number.chars().collect();
    if chars.len() <= 4 {
        return number.to_string();
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{}{tail}", "•".repeat(chars.len() - 4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_acct_030_and_240() {
        assert_eq!(
            AccountType::RothIra.default_tax_treatment(),
            TaxTreatment::TaxExempt
        );
        assert_eq!(
            AccountType::Retirement401k.default_tax_treatment(),
            TaxTreatment::TaxDeferred
        );
        assert_eq!(
            AccountType::Checking.default_tax_treatment(),
            TaxTreatment::Taxable
        );
        assert_eq!(AccountType::Loan.default_group(), AccountGroup::Liabilities);
        assert!(
            AccountFields::new("B", AccountType::Brokerage)
                .investment
                .is_some()
        );
        assert!(
            AccountFields::new("C", AccountType::Checking)
                .investment
                .is_none()
        );
    }

    #[test]
    fn masks_account_numbers() {
        assert_eq!(masked_account_number("123456789"), "•••••6789");
        assert_eq!(masked_account_number("1234"), "1234");
        assert_eq!(masked_account_number(""), "");
    }

    #[test]
    fn text_enum_round_trips() {
        for t in AccountType::ALL {
            assert_eq!(t.as_str().parse::<AccountType>().unwrap(), *t);
        }
        assert!("bogus".parse::<AccountType>().is_err());
    }
}
