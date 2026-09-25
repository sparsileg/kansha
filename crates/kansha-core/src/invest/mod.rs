//! Investments: transactions, lots, positions (INV-010 … INV-310,
//! LOT-010 … LOT-160, POS-010 … POS-050).
//!
//! An investment transaction is an ordinary `txn` whose postings carry its
//! cash and cost-basis effects (INV-040), plus an `investment_txn` row
//! with the trade detail and the lot records it made:
//!
//! - A posting to the investment account with a security carries that
//!   holding's cost basis; one without is the account's cash. With linked
//!   cash (INV-300) the cash posting goes to the linked account instead.
//! - Buy: cash −cost, holding +cost; a new lot. Sell: cash +proceeds,
//!   holding −basis, Realized Gain/Loss −(proceeds − basis); disposals.
//!   Income: cash +amount, income category −amount. Reinvest: holding
//!   +amount, income category −amount; a new lot. Return of capital: cash
//!   +amount, holding −basis reduced, Realized Gain/Loss −excess. Split:
//!   one zero posting to the holding; lot adjustments. Share transfer:
//!   holding −basis here, +basis there; lots move with their dates and
//!   basis. Shares added/removed: holding ± basis against Opening Balance.
//!
//! Lots are immutable acquisition facts; open quantity and basis come
//! from disposals and adjustments (spec §18).
//!
//! **Date order.** A holding's sales, transfers, removals, splits, and
//! returns of capital form a history in (date, entry) order. A new or
//! changed transaction that affects a holding's lots must come after
//! every such event already recorded for that holding; changing or
//! deleting one needs nothing after it. Memo and settlement date can
//! always change. Cash-only transactions (dividends, fees, cash
//! transfers) have no such limit.

mod lots;
mod reads;
mod seed;
mod service;

pub use reads::{
    Allocation, AllocationRow, Holdings, IncomeReport, IncomeRow, InvRegister, InvRegisterRow,
    LotView, PerfRow, Performance, Position, RealizedGain, account_value, allocation, holdings,
    income, open_lots, performance, realized_gains, register,
};
pub use seed::{SeedPreview, SeedRow, commit_seed, preview_seed};
pub use service::{create, delete, trade_amount, update};

pub(crate) use lots::OpenLot;
pub(crate) use service::Plan;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::accounts::{AccountId, LotMethod};
use crate::categories::SystemCategory;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::ledger::{Target, Txn, TxnId};
use crate::money::{Money, Price, Quantity};
use crate::persistence::invest as repo;
use crate::securities::SecurityId;
use crate::text_enum::text_enum;

/// Row ID of a lot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct LotId(pub i64);

text_enum! {
    /// Investment transaction types (INV-010).
    pub enum InvAction {
        Buy = "buy",
        Sell = "sell",
        Dividend = "dividend",
        Interest = "interest",
        ReinvestDividend = "reinvest_dividend",
        ReinvestCgShort = "reinvest_cg_short",
        ReinvestCgLong = "reinvest_cg_long",
        CgDistShort = "cg_dist_short",
        CgDistLong = "cg_dist_long",
        ReturnOfCapital = "return_of_capital",
        Split = "split",
        TransferShares = "transfer_shares",
        SharesAdded = "shares_added",
        SharesRemoved = "shares_removed",
        CashIn = "cash_in",
        CashOut = "cash_out",
        Fee = "fee",
        TaxWithholding = "tax_withholding",
        MiscIncome = "misc_income",
        MiscExpense = "misc_expense",
    }
}

text_enum! {
    /// Holding period (LOT-040): one year or less is short.
    pub enum Term {
        Short = "short",
        Long = "long",
    }
}

text_enum! {
    /// Why shares left a lot.
    pub enum DisposalKind {
        Sale = "sale",
        TransferOut = "transfer_out",
        Removed = "removed",
    }
}

text_enum! {
    /// A change to an open lot that is not a disposal.
    pub enum AdjustmentKind {
        Split = "split",
        ReturnOfCapital = "return_of_capital",
    }
}

impl InvAction {
    /// Needs a security (the schema's rule).
    pub const fn needs_security(self) -> bool {
        !matches!(
            self,
            InvAction::Interest
                | InvAction::CashIn
                | InvAction::CashOut
                | InvAction::Fee
                | InvAction::TaxWithholding
                | InvAction::MiscIncome
                | InvAction::MiscExpense
        )
    }

    /// May name a security (interest, fees, and the like optionally do).
    pub const fn allows_security(self) -> bool {
        !matches!(self, InvAction::CashIn | InvAction::CashOut)
    }

    /// Takes a number of shares.
    pub const fn takes_quantity(self) -> bool {
        matches!(
            self,
            InvAction::Buy
                | InvAction::Sell
                | InvAction::ReinvestDividend
                | InvAction::ReinvestCgShort
                | InvAction::ReinvestCgLong
                | InvAction::TransferShares
                | InvAction::SharesAdded
                | InvAction::SharesRemoved
        )
    }

    /// Creates a lot in this account (LOT-010).
    pub const fn acquires(self) -> bool {
        matches!(
            self,
            InvAction::Buy
                | InvAction::ReinvestDividend
                | InvAction::ReinvestCgShort
                | InvAction::ReinvestCgLong
                | InvAction::SharesAdded
        )
    }

    /// Takes shares out of lots, chosen by a lot selection method.
    pub const fn disposes(self) -> bool {
        matches!(
            self,
            InvAction::Sell | InvAction::TransferShares | InvAction::SharesRemoved
        )
    }

    /// Changes lots at all: its holding's history is in date order.
    pub const fn affects_lots(self) -> bool {
        self.acquires()
            || self.disposes()
            || matches!(self, InvAction::Split | InvAction::ReturnOfCapital)
    }

    /// Has an amount of money: everything but split and the share-only
    /// transfers and removals.
    pub const fn takes_amount(self) -> bool {
        !matches!(
            self,
            InvAction::Split | InvAction::TransferShares | InvAction::SharesRemoved
        )
    }

    /// Money into (+1) or out of (−1) the account's cash; 0 for none.
    pub const fn cash_direction(self) -> i8 {
        match self {
            InvAction::Sell
            | InvAction::Dividend
            | InvAction::Interest
            | InvAction::CgDistShort
            | InvAction::CgDistLong
            | InvAction::ReturnOfCapital
            | InvAction::CashIn
            | InvAction::MiscIncome => 1,
            InvAction::Buy
            | InvAction::CashOut
            | InvAction::Fee
            | InvAction::TaxWithholding
            | InvAction::MiscExpense => -1,
            _ => 0,
        }
    }

    /// The built-in category the amount goes to, if the action has one.
    pub const fn category(self) -> Option<SystemCategory> {
        Some(match self {
            InvAction::Dividend | InvAction::ReinvestDividend => SystemCategory::Dividends,
            InvAction::Interest => SystemCategory::Interest,
            InvAction::CgDistShort | InvAction::ReinvestCgShort => SystemCategory::CapGainDistShort,
            InvAction::CgDistLong | InvAction::ReinvestCgLong => SystemCategory::CapGainDistLong,
            InvAction::MiscIncome => SystemCategory::InvestmentIncome,
            InvAction::Fee => SystemCategory::InvestmentFees,
            InvAction::TaxWithholding => SystemCategory::TaxWithheld,
            InvAction::MiscExpense => SystemCategory::InvestmentExpense,
            _ => return None,
        })
    }

    /// Short label for the register's Action column (INV-030).
    pub const fn label(self) -> &'static str {
        match self {
            InvAction::Buy => "Buy",
            InvAction::Sell => "Sell",
            InvAction::Dividend => "Dividend",
            InvAction::Interest => "Interest",
            InvAction::ReinvestDividend => "Reinvest Div",
            InvAction::ReinvestCgShort => "Reinvest ST CG",
            InvAction::ReinvestCgLong => "Reinvest LT CG",
            InvAction::CgDistShort => "ST Cap Gain",
            InvAction::CgDistLong => "LT Cap Gain",
            InvAction::ReturnOfCapital => "Return of Capital",
            InvAction::Split => "Split",
            InvAction::TransferShares => "Transfer Shares",
            InvAction::SharesAdded => "Shares Added",
            InvAction::SharesRemoved => "Shares Removed",
            InvAction::CashIn => "Cash In",
            InvAction::CashOut => "Cash Out",
            InvAction::Fee => "Fee",
            InvAction::TaxWithholding => "Tax Withheld",
            InvAction::MiscIncome => "Misc Income",
            InvAction::MiscExpense => "Misc Expense",
        }
    }
}

/// A split ratio: `new` shares for every `old` (2:1 is new 2, old 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SplitRatio {
    pub new: i64,
    pub old: i64,
}

/// Shares to take from one lot (specific identification, LOT-100).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LotPick {
    pub lot: LotId,
    pub quantity: Quantity,
}

/// An investment transaction to be written (INV-010, INV-020). Fields an
/// action does not use must be left empty; the engine says which.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InvInput {
    pub account: AccountId,
    pub action: InvAction,
    /// Trade date.
    pub date: Date,
    pub settle_date: Option<Date>,
    pub security: Option<SecurityId>,
    pub quantity: Option<Quantity>,
    pub price: Option<Price>,
    /// Buy and sell only.
    pub commission: Money,
    /// Always positive; the engine gives it its sign. Buy: total cost
    /// with commission. Sell: proceeds after commission. Reinvest: the
    /// amount reinvested. Shares added: their cost basis. Otherwise the
    /// amount paid or received. For buy, sell, and reinvest it may be
    /// left out and is then shares × price (± commission).
    pub amount: Option<Money>,
    pub split: Option<SplitRatio>,
    /// Share transfers: the investment account receiving the shares.
    pub to_account: Option<AccountId>,
    /// Overrides the security's and account's lot selection method.
    pub lot_method: Option<LotMethod>,
    /// Specific identification: shares from each lot (LOT-100).
    pub lots: Vec<LotPick>,
    /// Shares added: the original acquisition date (MIG-120); the trade
    /// date when left out.
    pub acquired: Option<Date>,
    /// Cash in/out: the other account or a category. Misc income or
    /// expense: a category instead of the built-in one.
    pub counterpart: Option<Target>,
    pub memo: String,
}

impl InvInput {
    /// An input with only the essentials; fill in the rest.
    pub fn new(account: AccountId, action: InvAction, date: Date) -> InvInput {
        InvInput {
            account,
            action,
            date,
            settle_date: None,
            security: None,
            quantity: None,
            price: None,
            commission: Money::ZERO,
            amount: None,
            split: None,
            to_account: None,
            lot_method: None,
            lots: Vec::new(),
            acquired: None,
            counterpart: None,
            memo: String::new(),
        }
    }
}

/// A lot as recorded at acquisition (LOT-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Lot {
    pub id: LotId,
    pub account: AccountId,
    pub security: SecurityId,
    pub acquired: Date,
    pub quantity: Quantity,
    pub basis: Money,
    pub origin_txn: TxnId,
    /// The lot it came from, for shares transferred in (LOT-140).
    pub source_lot: Option<LotId>,
}

/// Shares leaving a lot. A sale is a realized gain record (LOT-040).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Disposal {
    pub lot: LotId,
    pub kind: DisposalKind,
    pub quantity: Quantity,
    pub basis: Money,
    pub proceeds: Option<Money>,
    pub gain: Option<Money>,
    pub term: Option<Term>,
}

/// A split or return of capital on one lot (LOT-120, LOT-130).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Adjustment {
    pub lot: LotId,
    pub kind: AdjustmentKind,
    pub quantity_delta: Quantity,
    pub basis_delta: Money,
}

/// A stored investment transaction: the ledger transaction with its
/// trade detail and lot records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InvTxn {
    #[serde(flatten)]
    pub txn: Txn,
    pub account: AccountId,
    pub action: InvAction,
    pub security: Option<SecurityId>,
    pub quantity: Option<Quantity>,
    pub price: Option<Price>,
    pub commission: Money,
    pub split: Option<SplitRatio>,
    pub to_account: Option<AccountId>,
    pub lot_method: Option<LotMethod>,
    pub settle_date: Option<Date>,
    /// The cash posting, as the cash account sees it (+ in, − out);
    /// zero when the action moves no cash.
    pub cash: Money,
    pub lots: Vec<Lot>,
    pub disposals: Vec<Disposal>,
    pub adjustments: Vec<Adjustment>,
}

impl InvTxn {
    /// The input that would write this transaction again, for editing.
    pub fn to_input(&self) -> InvInput {
        let holding = |account: AccountId| -> Money {
            self.txn
                .postings
                .iter()
                .filter(|p| p.target == Target::Account(account) && p.security.is_some())
                .map(|p| p.amount)
                .sum()
        };
        let amount = match self.action {
            InvAction::Split | InvAction::TransferShares | InvAction::SharesRemoved => None,
            InvAction::ReinvestDividend
            | InvAction::ReinvestCgShort
            | InvAction::ReinvestCgLong
            | InvAction::SharesAdded => Some(holding(self.account)),
            _ => self.cash.checked_neg().map(|n| n.max(self.cash)),
        };
        let counterpart = match self.action {
            InvAction::CashIn | InvAction::CashOut => self
                .txn
                .postings
                .iter()
                .find(|p| p.target != Target::Account(self.account))
                .map(|p| p.target),
            InvAction::MiscIncome | InvAction::MiscExpense => self
                .txn
                .postings
                .iter()
                .find(|p| matches!(p.target, Target::Category(_)))
                .map(|p| p.target),
            _ => None,
        };
        let lots = if self.lot_method == Some(LotMethod::Specific) {
            self.disposals
                .iter()
                .map(|d| LotPick {
                    lot: d.lot,
                    quantity: d.quantity,
                })
                .collect()
        } else {
            Vec::new()
        };
        InvInput {
            account: self.account,
            action: self.action,
            date: self.txn.date,
            settle_date: self.settle_date,
            security: self.security,
            quantity: self.quantity,
            price: self.price,
            commission: self.commission,
            amount,
            split: self.split,
            to_account: self.to_account,
            lot_method: self.lot_method,
            lots,
            acquired: match self.action {
                InvAction::SharesAdded => self.lots.first().map(|l| l.acquired),
                _ => None,
            },
            counterpart,
            memo: self.txn.memo.clone(),
        }
    }
}

/// One investment transaction by ID.
pub fn get(conn: &Connection, id: TxnId) -> Result<InvTxn> {
    repo::get(conn, id)
}

/// The investment transaction with this ID, if the transaction is one.
pub fn find(conn: &Connection, id: TxnId) -> Result<Option<InvTxn>> {
    repo::find(conn, id)
}

/// Fail unless `account` is an investment account.
pub(crate) fn investment_account(
    conn: &Connection,
    account: AccountId,
) -> Result<crate::accounts::Account> {
    let acct = crate::persistence::accounts::get(conn, account)?;
    if !acct.fields.account_type.is_investment() {
        return Err(Error::Invalid(format!(
            "{:?} is not an investment account",
            acct.fields.name
        )));
    }
    Ok(acct)
}
