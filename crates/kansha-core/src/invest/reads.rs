//! Investment reads: register (INV-030), holdings and positions (POS-010),
//! lots (LOT-150), realized gains (LOT-040, LOT-160), income, simple
//! performance (POS-030), and asset allocation (POS-020).
//!
//! Market value uses the latest price on or before the valuation date
//! (PRC-050); a money market fund with no price is worth $1.00 a share
//! (INV-050). A holding with no price has no market value and is flagged.

use std::collections::BTreeMap;

use rusqlite::Connection;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use super::lots::term;
use super::{InvAction, Lot, LotId, SplitRatio, Term, investment_account};
use crate::accounts::{AccountId, AccountStatus, CashMode, TaxTreatment};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::ledger::{Cleared, Target, TxnId};
use crate::money::{Money, Price, Quantity, extended_value};
use crate::persistence::{accounts, invest as repo, securities};
use crate::securities::{
    AssetClass, DEFAULT_STALE_DAYS, MONEY_MARKET_PRICE, Security, SecurityId, SecurityType,
};

// ---------------------------------------------------------------------------
// Register (INV-030)
// ---------------------------------------------------------------------------

/// One row of an investment account's register.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InvRegisterRow {
    pub txn_id: TxnId,
    pub date: Date,
    pub settle_date: Option<Date>,
    pub action: InvAction,
    /// For display: "Buy", "Reinvest Div", ... ("Transfer In" for shares
    /// arriving from another account).
    pub action_label: String,
    pub security: Option<SecurityId>,
    /// Ticker, or name if none.
    pub security_label: String,
    pub quantity: Option<Quantity>,
    pub price: Option<Price>,
    pub commission: Money,
    pub split: Option<SplitRatio>,
    /// Cash in (+) or out (−) of the account's cash (or its linked cash
    /// account); zero when none moves.
    pub amount: Money,
    /// Running cash balance, in date order; `None` with linked cash.
    pub cash_balance: Option<Money>,
    pub memo: String,
    /// The cash posting's status; `None` when there is none.
    pub cleared: Option<Cleared>,
    /// The other account: share transfers, and cash in/out.
    pub other_account: Option<AccountId>,
    /// A category the cash came from or went to (cash in/out, misc).
    pub other_category: Option<crate::categories::CategoryId>,
    /// Shares arriving here from another account.
    pub incoming: bool,
    /// Dated after today (REG-070).
    pub future: bool,
}

/// An investment account's register.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InvRegister {
    pub account: AccountId,
    pub rows: Vec<InvRegisterRow>,
    /// Cash as of today; `None` with linked cash (INV-300).
    pub cash: Option<Money>,
    /// Cash below zero is allowed but flagged (INV-310).
    pub negative_cash: bool,
    pub today: Date,
}

fn internal_cash(conn: &Connection, account: AccountId) -> Result<bool> {
    let acct = investment_account(conn, account)?;
    Ok(acct
        .fields
        .investment
        .as_ref()
        .is_some_and(|i| i.cash_mode == CashMode::Internal))
}

fn labels(conn: &Connection) -> Result<BTreeMap<SecurityId, Security>> {
    Ok(securities::list(conn)?
        .into_iter()
        .map(|s| (s.id, s))
        .collect())
}

/// Every transaction in or into `account`, oldest first.
pub fn register(conn: &Connection, account: AccountId, today: Date) -> Result<InvRegister> {
    let internal = internal_cash(conn, account)?;
    let secs = labels(conn)?;
    let mut rows = Vec::new();
    let mut running = Money::ZERO;
    for id in repo::register_ids(conn, account)? {
        let t = repo::get(conn, id)?;
        let incoming = t.to_account == Some(account);
        let amount = if incoming { Money::ZERO } else { t.cash };
        running = running
            .checked_add(amount)
            .ok_or(Error::Overflow("cash balance"))?;
        let cash_account = repo::cash_account(conn, t.account)?;
        let cash_posting = t
            .txn
            .postings
            .iter()
            .find(|p| p.target == Target::Account(cash_account) && p.security.is_none());
        let others = t.txn.postings.iter().filter(|p| {
            p.target != Target::Account(t.account) && p.target != Target::Account(cash_account)
        });
        let mut other_account = if incoming {
            Some(t.account)
        } else {
            t.to_account
        };
        let mut other_category = None;
        if matches!(
            t.action,
            InvAction::CashIn | InvAction::CashOut | InvAction::MiscIncome | InvAction::MiscExpense
        ) {
            for p in others {
                match p.target {
                    Target::Account(a) => other_account = Some(a),
                    Target::Category(c) => other_category = Some(c),
                }
            }
        }
        rows.push(InvRegisterRow {
            txn_id: t.txn.id,
            date: t.txn.date,
            settle_date: t.settle_date,
            action: t.action,
            action_label: if incoming {
                "Transfer In".into()
            } else if t.action == InvAction::TransferShares {
                "Transfer Out".into()
            } else {
                t.action.label().into()
            },
            security: t.security,
            security_label: t
                .security
                .and_then(|s| secs.get(&s))
                .map_or(String::new(), |s| s.label().to_string()),
            quantity: t.quantity,
            price: t.price,
            commission: t.commission,
            split: t.split,
            amount,
            cash_balance: internal.then_some(running),
            memo: t.txn.memo.clone(),
            cleared: if incoming {
                None
            } else {
                cash_posting.map(|p| p.cleared)
            },
            other_account,
            other_category,
            incoming,
            future: t.txn.date > today,
        });
    }
    let cash = if internal {
        Some(repo::cash_balance(conn, account, Some(today))?)
    } else {
        None
    };
    Ok(InvRegister {
        account,
        rows,
        negative_cash: cash.is_some_and(|c| c.is_negative()),
        cash,
        today,
    })
}

// ---------------------------------------------------------------------------
// Prices and positions (POS-010, PRC-050)
// ---------------------------------------------------------------------------

/// The price a holding is valued at: (price, price date, stale).
fn valuation(
    conn: &Connection,
    s: &Security,
    as_of: Date,
    stale_days: i64,
) -> Result<Option<(Price, Option<Date>, bool)>> {
    match securities::latest_price(conn, s.id, as_of)? {
        Some(p) => {
            let age = as_of
                .naive()
                .signed_duration_since(p.date.naive())
                .num_days();
            Ok(Some((p.price, Some(p.date), age > stale_days)))
        }
        None if s.fields.security_type == SecurityType::MoneyMarket => {
            Ok(Some((MONEY_MARKET_PRICE, None, false)))
        }
        None => Ok(None),
    }
}

/// One security held in one account (POS-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Position {
    pub account: AccountId,
    pub security: SecurityId,
    pub name: String,
    pub ticker: Option<String>,
    pub security_type: SecurityType,
    pub asset_class: AssetClass,
    pub shares: Quantity,
    pub basis: Money,
    /// Latest price on or before the valuation date; `None` if there is
    /// none.
    pub price: Option<Price>,
    /// `None` for a money market fund valued at $1.00 without a price.
    pub price_date: Option<Date>,
    /// The price is older than the stale threshold (PRC-050).
    pub stale: bool,
    pub market_value: Option<Money>,
    pub unrealized: Option<Money>,
}

/// Positions on `as_of` in `account` (or every account), by account and
/// security name.
fn positions(
    conn: &Connection,
    account: Option<AccountId>,
    as_of: Date,
    stale_days: i64,
) -> Result<Vec<Position>> {
    let secs = labels(conn)?;
    let mut grouped: BTreeMap<(AccountId, SecurityId), (Quantity, Money)> = BTreeMap::new();
    for l in repo::open_lots(conn, account, None, as_of)? {
        let e = grouped
            .entry((l.lot.account, l.lot.security))
            .or_insert((Quantity::ZERO, Money::ZERO));
        e.0 =
            e.0.checked_add(l.open_quantity)
                .ok_or(Error::Overflow("position shares"))?;
        e.1 =
            e.1.checked_add(l.open_basis)
                .ok_or(Error::Overflow("position basis"))?;
    }
    let mut out = Vec::with_capacity(grouped.len());
    for ((acct, sec), (shares, basis)) in grouped {
        let s = secs.get(&sec).ok_or(Error::NotFound {
            entity: "security",
            id: sec.0,
        })?;
        let val = valuation(conn, s, as_of, stale_days)?;
        let market_value = val.map(|(p, _, _)| extended_value(shares, p)).transpose()?;
        out.push(Position {
            account: acct,
            security: sec,
            name: s.fields.name.clone(),
            ticker: s.fields.ticker.clone(),
            security_type: s.fields.security_type,
            asset_class: s.fields.asset_class,
            shares,
            basis,
            price: val.map(|v| v.0),
            price_date: val.and_then(|v| v.1),
            stale: val.is_some_and(|v| v.2),
            unrealized: market_value.and_then(|mv| mv.checked_sub(basis)),
            market_value,
        });
    }
    out.sort_by(|a, b| {
        (a.account, a.name.to_lowercase(), a.security).cmp(&(
            b.account,
            b.name.to_lowercase(),
            b.security,
        ))
    });
    Ok(out)
}

/// An investment account's holdings (POS-010, POS-040 Holdings tab).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Holdings {
    pub account: AccountId,
    pub as_of: Date,
    pub positions: Vec<Position>,
    /// `None` with linked cash.
    pub cash: Option<Money>,
    /// Σ cost basis of the positions.
    pub basis: Money,
    /// Σ market value of the positions that have a price.
    pub market_value: Money,
    /// Market value plus cash.
    pub total_value: Money,
    /// Σ unrealized gain of the positions that have a price.
    pub unrealized: Money,
    /// Some position has no price, so the totals leave it out.
    pub missing_prices: bool,
    pub stale_prices: bool,
}

/// `account`'s holdings on `as_of`. `stale_days` defaults to
/// [`DEFAULT_STALE_DAYS`].
pub fn holdings(
    conn: &Connection,
    account: AccountId,
    as_of: Date,
    stale_days: Option<i64>,
) -> Result<Holdings> {
    let internal = internal_cash(conn, account)?;
    let positions = positions(
        conn,
        Some(account),
        as_of,
        stale_days.unwrap_or(DEFAULT_STALE_DAYS),
    )?;
    let cash = if internal {
        Some(repo::cash_balance(conn, account, Some(as_of))?)
    } else {
        None
    };
    let sum = |f: &dyn Fn(&Position) -> Option<Money>| -> Result<Money> {
        positions
            .iter()
            .filter_map(f)
            .try_fold(Money::ZERO, |a, m| {
                a.checked_add(m).ok_or(Error::Overflow("holdings total"))
            })
    };
    let basis = sum(&|p| Some(p.basis))?;
    let market_value = sum(&|p| p.market_value)?;
    let unrealized = sum(&|p| p.unrealized)?;
    Ok(Holdings {
        account,
        as_of,
        total_value: market_value
            .checked_add(cash.unwrap_or(Money::ZERO))
            .ok_or(Error::Overflow("holdings total"))?,
        cash,
        basis,
        market_value,
        unrealized,
        missing_prices: positions.iter().any(|p| p.market_value.is_none()),
        stale_prices: positions.iter().any(|p| p.stale),
        positions,
    })
}

/// What an investment account is worth on `as_of` for the account list:
/// cash plus market value, a holding with no price counted at cost.
pub fn account_value(conn: &Connection, account: AccountId, as_of: Date) -> Result<Money> {
    let mut total = repo::cash_balance(conn, account, Some(as_of))?;
    for p in positions(conn, Some(account), as_of, DEFAULT_STALE_DAYS)? {
        total = total
            .checked_add(p.market_value.unwrap_or(p.basis))
            .ok_or(Error::Overflow("account value"))?;
    }
    Ok(total)
}

// ---------------------------------------------------------------------------
// Lots (LOT-150)
// ---------------------------------------------------------------------------

/// An open lot, valued (LOT-150).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LotView {
    #[serde(flatten)]
    pub lot: Lot,
    pub security_label: String,
    pub open_quantity: Quantity,
    pub open_basis: Money,
    /// Open basis ÷ open shares.
    pub per_share: Option<Price>,
    pub market_value: Option<Money>,
    pub unrealized: Option<Money>,
    /// Holding period if sold on the valuation date.
    pub term: Term,
}

/// Open lots of `account` (one security or all) on `as_of`, oldest
/// acquisition first within each security.
pub fn open_lots(
    conn: &Connection,
    account: AccountId,
    security: Option<SecurityId>,
    as_of: Date,
) -> Result<Vec<LotView>> {
    investment_account(conn, account)?;
    let secs = labels(conn)?;
    let mut out = Vec::new();
    for l in repo::open_lots(conn, Some(account), security, as_of)? {
        let s = secs.get(&l.lot.security).ok_or(Error::NotFound {
            entity: "security",
            id: l.lot.security.0,
        })?;
        let market_value = valuation(conn, s, as_of, DEFAULT_STALE_DAYS)?
            .map(|(p, _, _)| extended_value(l.open_quantity, p))
            .transpose()?;
        out.push(LotView {
            security_label: s.label().to_string(),
            per_share: Price::per_share(l.open_basis, l.open_quantity)?,
            unrealized: market_value.and_then(|mv| mv.checked_sub(l.open_basis)),
            market_value,
            term: term(l.lot.acquired, as_of),
            open_quantity: l.open_quantity,
            open_basis: l.open_basis,
            lot: l.lot,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Realized gains (LOT-040, LOT-130, LOT-160)
// ---------------------------------------------------------------------------

/// One realized gain or loss.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RealizedGain {
    pub txn_id: TxnId,
    pub account: AccountId,
    pub security: SecurityId,
    pub security_label: String,
    /// `None` for return of capital beyond basis (LOT-130), which has no
    /// lot record.
    pub lot: Option<LotId>,
    pub sale_date: Date,
    pub acquired: Option<Date>,
    pub quantity: Option<Quantity>,
    pub proceeds: Money,
    pub basis: Money,
    pub gain: Money,
    /// `None` for return of capital beyond basis.
    pub term: Option<Term>,
    /// From a taxable account; gains in tax-deferred and tax-exempt
    /// accounts are kept but left out of taxable totals (LOT-160).
    pub taxable: bool,
}

/// Realized gains dated from `from` to `to` (either open), in `account`
/// or every account, by date.
pub fn realized_gains(
    conn: &Connection,
    account: Option<AccountId>,
    from: Option<Date>,
    to: Option<Date>,
) -> Result<Vec<RealizedGain>> {
    let secs = labels(conn)?;
    let taxable: BTreeMap<AccountId, bool> = accounts::list(conn)?
        .into_iter()
        .map(|a| (a.id, a.fields.tax_treatment == TaxTreatment::Taxable))
        .collect();
    let label = |s: SecurityId| {
        secs.get(&s)
            .map_or(String::new(), |x| x.label().to_string())
    };
    let mut out: Vec<RealizedGain> = repo::sales(conn, account, from, to)?
        .into_iter()
        .map(|r| RealizedGain {
            txn_id: r.txn,
            account: r.account,
            security: r.security,
            security_label: label(r.security),
            lot: Some(r.lot),
            sale_date: r.sale_date,
            acquired: Some(r.acquired),
            quantity: Some(r.quantity),
            proceeds: r.proceeds,
            basis: r.basis,
            gain: r.gain,
            term: Some(r.term),
            taxable: taxable.get(&r.account).copied().unwrap_or(true),
        })
        .collect();
    for c in repo::category_amounts(conn, account, from, to)? {
        let (InvAction::ReturnOfCapital, Some("realized_gain"), Some(sec)) =
            (c.action, c.system_key.as_deref(), c.security)
        else {
            continue;
        };
        let gain = c
            .amount
            .checked_neg()
            .ok_or(Error::Overflow("realized gain"))?;
        out.push(RealizedGain {
            txn_id: c.txn,
            account: c.account,
            security: sec,
            security_label: label(sec),
            lot: None,
            sale_date: c.date,
            acquired: None,
            quantity: None,
            proceeds: gain,
            basis: Money::ZERO,
            gain,
            term: None,
            taxable: taxable.get(&c.account).copied().unwrap_or(true),
        });
    }
    out.sort_by_key(|g| (g.sale_date, g.txn_id));
    Ok(out)
}

// ---------------------------------------------------------------------------
// Income (POS-010, POS-040 Income tab)
// ---------------------------------------------------------------------------

/// Investment income from one security (or none: interest and other
/// income not tied to a security).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct IncomeRow {
    pub security: Option<SecurityId>,
    pub security_label: String,
    /// Cash and reinvested dividends.
    pub dividends: Money,
    pub interest: Money,
    pub cg_short: Money,
    pub cg_long: Money,
    /// Miscellaneous income.
    pub other: Money,
    pub total: Money,
}

impl IncomeRow {
    fn add(&mut self, action: InvAction, amount: Money) -> Result<()> {
        let slot = match action {
            InvAction::Dividend | InvAction::ReinvestDividend => &mut self.dividends,
            InvAction::Interest => &mut self.interest,
            InvAction::CgDistShort | InvAction::ReinvestCgShort => &mut self.cg_short,
            InvAction::CgDistLong | InvAction::ReinvestCgLong => &mut self.cg_long,
            InvAction::MiscIncome => &mut self.other,
            _ => return Ok(()),
        };
        let overflow = || Error::Overflow("income total");
        *slot = slot.checked_add(amount).ok_or_else(overflow)?;
        self.total = self.total.checked_add(amount).ok_or_else(overflow)?;
        Ok(())
    }
}

/// Income by security, with totals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct IncomeReport {
    pub rows: Vec<IncomeRow>,
    pub total: IncomeRow,
}

/// `account`'s investment income dated from `from` to `to`.
pub fn income(
    conn: &Connection,
    account: AccountId,
    from: Option<Date>,
    to: Option<Date>,
) -> Result<IncomeReport> {
    investment_account(conn, account)?;
    let secs = labels(conn)?;
    let mut by: BTreeMap<Option<SecurityId>, IncomeRow> = BTreeMap::new();
    let mut total = IncomeRow::default();
    for c in repo::category_amounts(conn, Some(account), from, to)? {
        if c.system_key.as_deref() == Some("realized_gain") {
            continue;
        }
        let amount = c.amount.checked_neg().ok_or(Error::Overflow("income"))?;
        let row = by.entry(c.security).or_insert_with(|| IncomeRow {
            security: c.security,
            security_label: c
                .security
                .and_then(|s| secs.get(&s))
                .map_or(String::new(), |s| s.label().to_string()),
            ..IncomeRow::default()
        });
        row.add(c.action, amount)?;
        total.add(c.action, amount)?;
    }
    let mut rows: Vec<IncomeRow> = by.into_values().filter(|r| !r.total.is_zero()).collect();
    rows.sort_by(|a, b| {
        (a.security.is_none(), a.security_label.to_lowercase())
            .cmp(&(b.security.is_none(), b.security_label.to_lowercase()))
    });
    Ok(IncomeReport { rows, total })
}

// ---------------------------------------------------------------------------
// Performance (POS-030, simple measures)
// ---------------------------------------------------------------------------

/// Simple performance of one security in an account, or the account's
/// total (`security` `None`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PerfRow {
    pub security: Option<SecurityId>,
    pub security_label: String,
    /// Open cost basis.
    pub basis: Money,
    pub market_value: Option<Money>,
    pub unrealized: Option<Money>,
    pub realized: Money,
    pub income: Money,
    /// Unrealized + realized + income.
    pub total_gain: Option<Money>,
    /// Total gain ÷ (open basis + basis of shares sold), in percent, two
    /// decimals.
    pub total_return: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Performance {
    pub account: AccountId,
    pub as_of: Date,
    pub rows: Vec<PerfRow>,
    pub total: PerfRow,
}

/// `part ÷ whole` in percent, two decimals, half-even.
fn percent(part: Money, whole: Money) -> Option<String> {
    if whole.cents() <= 0 {
        return None;
    }
    let p = (part.to_decimal() * Decimal::ONE_HUNDRED)
        .checked_div(whole.to_decimal())?
        .round_dp_with_strategy(2, RoundingStrategy::MidpointNearestEven);
    Some(format!("{p:.2}"))
}

#[derive(Default)]
struct PerfSums {
    basis: Money,
    market_value: Option<Money>,
    priced: bool,
    realized: Money,
    sold_basis: Money,
    income: Money,
}

impl PerfSums {
    fn row(&self, security: Option<SecurityId>, label: String) -> Result<PerfRow> {
        let unrealized = match (self.priced, self.market_value) {
            (true, Some(mv)) => Some(mv.checked_sub(self.basis).ok_or(Error::Overflow("perf"))?),
            _ => None,
        };
        let market_value = if self.priced { self.market_value } else { None };
        let known = self
            .realized
            .checked_add(self.income)
            .ok_or(Error::Overflow("perf"))?;
        let total_gain = unrealized.and_then(|u| u.checked_add(known));
        let invested = self
            .basis
            .checked_add(self.sold_basis)
            .ok_or(Error::Overflow("perf"))?;
        Ok(PerfRow {
            security,
            security_label: label,
            basis: self.basis,
            market_value,
            unrealized,
            realized: self.realized,
            income: self.income,
            total_return: total_gain.and_then(|g| percent(g, invested)),
            total_gain,
        })
    }
}

/// Simple performance of `account` on `as_of` (POS-030): per security
/// ever held, and in total.
pub fn performance(conn: &Connection, account: AccountId, as_of: Date) -> Result<Performance> {
    investment_account(conn, account)?;
    let secs = labels(conn)?;
    let mut by: BTreeMap<Option<SecurityId>, PerfSums> = BTreeMap::new();
    let mut add = |s: Option<SecurityId>, f: &dyn Fn(&mut PerfSums) -> Result<()>| -> Result<()> {
        let e = by.entry(s).or_insert_with(|| PerfSums {
            priced: true,
            market_value: Some(Money::ZERO),
            ..PerfSums::default()
        });
        f(e)
    };
    let overflow = || Error::Overflow("performance");
    for p in positions(conn, Some(account), as_of, DEFAULT_STALE_DAYS)? {
        add(Some(p.security), &|e| {
            e.basis = e.basis.checked_add(p.basis).ok_or_else(overflow)?;
            match (e.market_value, p.market_value) {
                (Some(a), Some(b)) => {
                    e.market_value = Some(a.checked_add(b).ok_or_else(overflow)?);
                }
                _ => e.priced = false,
            }
            Ok(())
        })?;
    }
    for g in realized_gains(conn, Some(account), None, Some(as_of))? {
        add(Some(g.security), &|e| {
            e.realized = e.realized.checked_add(g.gain).ok_or_else(overflow)?;
            e.sold_basis = e.sold_basis.checked_add(g.basis).ok_or_else(overflow)?;
            Ok(())
        })?;
    }
    for r in income(conn, account, None, Some(as_of))?.rows {
        add(r.security, &|e| {
            e.income = e.income.checked_add(r.total).ok_or_else(overflow)?;
            Ok(())
        })?;
    }

    let mut total = PerfSums {
        priced: true,
        market_value: Some(Money::ZERO),
        ..PerfSums::default()
    };
    let mut rows = Vec::with_capacity(by.len());
    for (s, e) in &by {
        total.basis = total.basis.checked_add(e.basis).ok_or_else(overflow)?;
        total.realized = total
            .realized
            .checked_add(e.realized)
            .ok_or_else(overflow)?;
        total.sold_basis = total
            .sold_basis
            .checked_add(e.sold_basis)
            .ok_or_else(overflow)?;
        total.income = total.income.checked_add(e.income).ok_or_else(overflow)?;
        match (total.market_value, e.market_value, e.priced) {
            (Some(a), Some(b), true) => {
                total.market_value = Some(a.checked_add(b).ok_or_else(overflow)?);
            }
            _ => total.priced = false,
        }
        let label = s
            .and_then(|s| secs.get(&s))
            .map_or_else(|| "Other".to_string(), |x| x.label().to_string());
        rows.push(e.row(*s, label)?);
    }
    rows.sort_by(|a, b| {
        (a.security.is_none(), a.security_label.to_lowercase())
            .cmp(&(b.security.is_none(), b.security_label.to_lowercase()))
    });
    Ok(Performance {
        account,
        as_of,
        rows,
        total: total.row(None, "Total".into())?,
    })
}

// ---------------------------------------------------------------------------
// Asset allocation (POS-020)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AllocationRow {
    pub asset_class: AssetClass,
    pub market_value: Money,
    /// Share of the total, percent with two decimals.
    pub percent: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Allocation {
    pub as_of: Date,
    pub accounts: Vec<AccountId>,
    pub rows: Vec<AllocationRow>,
    pub total: Money,
    /// Holdings with no price are left out.
    pub missing_prices: bool,
}

/// Market value by asset class across `accounts` (every open investment
/// account when empty) on `as_of`. Investment cash counts as Cash.
pub fn allocation(conn: &Connection, accounts_: &[AccountId], as_of: Date) -> Result<Allocation> {
    let chosen: Vec<AccountId> = if accounts_.is_empty() {
        accounts::list(conn)?
            .into_iter()
            .filter(|a| a.fields.account_type.is_investment() && a.status == AccountStatus::Open)
            .map(|a| a.id)
            .collect()
    } else {
        for a in accounts_ {
            investment_account(conn, *a)?;
        }
        accounts_.to_vec()
    };
    let overflow = || Error::Overflow("allocation");
    let mut by: BTreeMap<&'static str, (AssetClass, Money)> = BTreeMap::new();
    let mut missing = false;
    let mut add = |class: AssetClass, m: Money| -> Result<()> {
        let e = by.entry(class.as_str()).or_insert((class, Money::ZERO));
        e.1 = e.1.checked_add(m).ok_or_else(overflow)?;
        Ok(())
    };
    for &a in &chosen {
        if internal_cash(conn, a)? {
            add(AssetClass::Cash, repo::cash_balance(conn, a, Some(as_of))?)?;
        }
        for p in positions(conn, Some(a), as_of, DEFAULT_STALE_DAYS)? {
            match p.market_value {
                Some(mv) => add(p.asset_class, mv)?,
                None => missing = true,
            }
        }
    }
    let total = by.values().try_fold(Money::ZERO, |t, (_, m)| {
        t.checked_add(*m).ok_or_else(overflow)
    })?;
    let mut rows: Vec<AllocationRow> = by
        .into_values()
        .filter(|(_, m)| !m.is_zero())
        .map(|(asset_class, market_value)| AllocationRow {
            asset_class,
            market_value,
            percent: percent(market_value, total).unwrap_or_else(|| "0.00".into()),
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.market_value));
    Ok(Allocation {
        as_of,
        accounts: chosen,
        rows,
        total,
        missing_prices: missing,
    })
}
