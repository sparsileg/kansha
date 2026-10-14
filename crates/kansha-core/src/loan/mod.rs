//! Loans (LOAN-010 … LOAN-090): stored terms on a Loan/Mortgage
//! account. The register holds the real balance; the terms split each
//! payment into principal and interest and drive the payment reminder
//! (REC-200).

pub mod math;
mod service;

pub use service::{
    LoanDetails, LoanPayment, LoanSave, LoanSaved, LoanView, details, lines_for, occurrence_lines,
    payment_for, preview, recurrence, save,
};
pub(crate) use service::{refresh, remove};

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;
use crate::categories::{CategoryId, PayeeId};
use crate::date::Date;
use crate::money::{Money, Rate};
use crate::schedule::ScheduleId;
use crate::text_enum::text_enum;

text_enum! {
    /// What the loan is for (LOAN-010).
    pub enum LoanType {
        Mortgage = "mortgage",
        Auto = "auto",
        Consumer = "consumer",
        Student = "student",
        Commercial = "commercial",
        Business = "business",
        Construction = "construction",
        HomeEquity = "home_equity",
    }
}

text_enum! {
    /// How often interest compounds (LOAN-020).
    pub enum Compounding {
        Daily = "daily",
        Monthly = "monthly",
        SemiAnnually = "semi_annually",
    }
}

impl Compounding {
    /// m: times a year.
    pub const fn per_year(self) -> i64 {
        match self {
            Compounding::Daily => 365,
            Compounding::Monthly => 12,
            Compounding::SemiAnnually => 2,
        }
    }
}

text_enum! {
    /// How often payments are due (LOAN-010).
    pub enum PaymentFrequency {
        TwicePerYear = "twice_per_year",
        Quarterly = "quarterly",
        EveryOtherMonth = "every_other_month",
        Monthly = "monthly",
        TwicePerMonth = "twice_per_month",
        EveryOtherWeek = "every_other_week",
        Weekly = "weekly",
    }
}

impl PaymentFrequency {
    /// p: payments a year.
    pub const fn per_year(self) -> i64 {
        match self {
            PaymentFrequency::TwicePerYear => 2,
            PaymentFrequency::Quarterly => 4,
            PaymentFrequency::EveryOtherMonth => 6,
            PaymentFrequency::Monthly => 12,
            PaymentFrequency::TwicePerMonth => 24,
            PaymentFrequency::EveryOtherWeek => 26,
            PaymentFrequency::Weekly => 52,
        }
    }
}

/// An interest rate and the date it takes effect (LOAN-050).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LoanRate {
    pub effective: Date,
    /// Annual, in percent.
    pub rate: Rate,
}

/// A fee, escrow, or insurance amount paid with each payment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LoanOther {
    pub category: CategoryId,
    /// Positive: what is paid.
    pub amount: Money,
}

/// A loan's terms as stored (LOAN-010, LOAN-015, LOAN-090).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LoanFields {
    /// The Loan/Mortgage account.
    pub account: AccountId,
    pub loan_type: LoanType,
    pub opening_date: Date,
    /// Positive: what was borrowed.
    pub original_balance: Money,
    pub length_months: i64,
    pub compounding: Compounding,
    pub frequency: PaymentFrequency,
    /// Twice per month: its two days, `day1 < day2`.
    pub day1: Option<i64>,
    pub day2: Option<i64>,
    /// Sorted by date; the first takes effect on the opening date.
    pub rates: Vec<LoanRate>,
    /// The lender's payment (principal and interest) as typed; `None`
    /// works it out from the terms (LOAN-025).
    pub payment: Option<Money>,
    /// Paid toward principal with each payment.
    pub extra_principal: Money,
    pub others: Vec<LoanOther>,
    /// The reminder's payee.
    pub lender: Option<PayeeId>,
    /// The account payments come from.
    pub paying_account: AccountId,
    /// Where interest goes (an expense category).
    pub interest_category: CategoryId,
}

/// A stored loan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Loan {
    pub fields: LoanFields,
    /// The payment reminder the loan owns (LOAN-060).
    pub schedule: Option<ScheduleId>,
}
