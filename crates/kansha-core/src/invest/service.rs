//! Investment writes: validate an [`InvInput`], work out its postings and
//! lot records (a [`Plan`]), and store them in one audited change inside
//! the caller's [`Tx`] (INT-020).

use std::collections::HashMap;

use rusqlite::Connection;

use super::lots::{self, OpenLot, Take};
use super::{
    Adjustment, AdjustmentKind, DisposalKind, InvAction, InvInput, InvTxn, LotId, Term,
    investment_account,
};
use crate::accounts::{AccountId, AccountStatus, CashMode, LotMethod, MmfMode};
use crate::categories::SystemCategory;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::ledger::{Cleared, PostingInput, Target, TxnId, TxnInput, TxnSource};
use crate::money::{Money, Quantity, extended_value};
use crate::persistence::{Origin, Tx, accounts, categories, invest as repo, securities};
use crate::securities::{Security, SecurityId, SecurityType};

/// A new lot to record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedLot {
    pub account: AccountId,
    pub security: SecurityId,
    pub acquired: Date,
    pub quantity: Quantity,
    pub basis: Money,
    pub source: Option<LotId>,
}

/// Shares to take out of an existing lot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedDisposal {
    pub lot: LotId,
    pub kind: DisposalKind,
    pub quantity: Quantity,
    pub basis: Money,
    pub proceeds: Option<Money>,
    pub term: Option<Term>,
}

/// Everything one investment transaction writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plan {
    pub txn: TxnInput,
    pub input: InvInput,
    /// The method used to choose lots (disposals only).
    pub lot_method: Option<LotMethod>,
    pub lots: Vec<PlannedLot>,
    pub disposals: Vec<PlannedDisposal>,
    pub adjustments: Vec<Adjustment>,
}

/// Create an investment transaction. Its source follows the write's
/// origin, as for ledger transactions.
pub fn create(tx: &Tx<'_>, input: &InvInput) -> Result<InvTxn> {
    let source = match tx.origin() {
        Origin::Ui => TxnSource::Manual,
        Origin::Import(batch) => TxnSource::Import { batch },
        Origin::System => TxnSource::System,
        Origin::Scheduler => {
            return Err(Error::Invalid(
                "investment transactions are not scheduled".into(),
            ));
        }
    };
    let plan = plan(tx.conn(), input, None)?;
    repo::insert(tx, source, &plan)
}

/// Replace an investment transaction. Memo and settlement date can always
/// change; anything that changes lots needs nothing after it in its
/// holding's history (see the module notes). A reconciled cash posting
/// needs `confirmed` and keeps its reconciliation when its account stays.
pub fn update(tx: &Tx<'_>, id: TxnId, input: &InvInput, confirmed: bool) -> Result<InvTxn> {
    let conn = tx.conn();
    let before = repo::get(conn, id)?;
    check_changeable(conn, &before, confirmed)?;
    let mut same = before.to_input();
    same.memo.clone_from(&input.memo);
    same.settle_date = input.settle_date;
    if same == *input {
        check_settle(input)?;
        return repo::update_header(tx, &before, &input.memo, input.settle_date);
    }
    if before.action.affects_lots() {
        check_nothing_after(conn, &before)?;
    }
    repo::clear_effects(tx, id)?;
    let mut plan = plan(tx.conn(), input, Some(&before))?;
    // A cash posting that stays in the same account keeps its cleared
    // status and reconciliation (TXN-050).
    let mut links: HashMap<AccountId, Option<i64>> = HashMap::new();
    for old in before.txn.postings.iter().filter(|p| p.security.is_none()) {
        let Target::Account(account) = old.target else {
            continue;
        };
        if let Some(p) = plan
            .txn
            .postings
            .iter_mut()
            .find(|p| p.target == old.target && p.security.is_none())
        {
            p.cleared = old.cleared;
            if old.cleared == Cleared::Reconciled {
                links.insert(account, old.reconciliation_id);
            }
        }
    }
    repo::rewrite(tx, &before, &plan, &links)
}

/// Delete an investment transaction and its lot records. One that changed
/// lots needs nothing after it in its holding's history.
pub fn delete(tx: &Tx<'_>, id: TxnId, confirmed: bool) -> Result<()> {
    let conn = tx.conn();
    let before = repo::get(conn, id)?;
    check_changeable(conn, &before, confirmed)?;
    if before.action.affects_lots() {
        check_nothing_after(conn, &before)?;
    }
    repo::delete(tx, &before)
}

/// The amount a trade comes to, for the entry form: buy, shares × price
/// + commission; sell, shares × price − commission; reinvest, shares ×
///   price. The UI does no money arithmetic, so it asks here.
pub fn trade_amount(
    action: InvAction,
    quantity: Quantity,
    price: crate::money::Price,
    commission: Money,
) -> Result<Money> {
    let value = extended_value(quantity, price)?;
    let overflow = || Error::Overflow("trade amount");
    match action {
        InvAction::Buy => value.checked_add(commission).ok_or_else(overflow),
        InvAction::Sell => value.checked_sub(commission).ok_or_else(overflow),
        InvAction::ReinvestDividend | InvAction::ReinvestCgShort | InvAction::ReinvestCgLong => {
            Ok(value)
        }
        other => Err(Error::Invalid(format!(
            "{} has no shares × price amount",
            other.label()
        ))),
    }
}

// ---------------------------------------------------------------------------
// Rules for existing transactions
// ---------------------------------------------------------------------------

fn check_changeable(conn: &Connection, t: &InvTxn, confirmed: bool) -> Result<()> {
    let mut touched: Vec<AccountId> = t.txn.accounts().collect();
    touched.push(t.account);
    touched.extend(t.to_account);
    for account in touched {
        let acct = accounts::get(conn, account)?;
        if acct.status == AccountStatus::Closed {
            return Err(Error::Invalid(format!(
                "account {:?} is closed; reopen it to change its transactions",
                acct.fields.name
            )));
        }
    }
    if t.txn.is_reconciled() && !confirmed {
        return Err(Error::ConfirmationRequired(format!(
            "transaction {} is reconciled",
            t.txn.id.0
        )));
    }
    Ok(())
}

/// The holdings a transaction's lot records belong to.
fn positions(t: &InvTxn) -> Vec<(AccountId, SecurityId)> {
    let Some(s) = t.security else {
        return Vec::new();
    };
    let mut out = vec![(t.account, s)];
    if let Some(to) = t.to_account {
        out.push((to, s));
    }
    out
}

/// Nothing sold, transferred, removed, split, or adjusted in `t`'s
/// holdings after `t` (date, then entry order).
fn check_nothing_after(conn: &Connection, t: &InvTxn) -> Result<()> {
    check_order(conn, &positions(t), t.txn.date, Some(t.txn.id))
}

/// Every lot event already recorded for `positions` (other than
/// `this`'s own) comes before (`date`, `this`); a new transaction comes
/// after everything entered so far.
fn check_order(
    conn: &Connection,
    positions: &[(AccountId, SecurityId)],
    date: Date,
    this: Option<TxnId>,
) -> Result<()> {
    for &(account, security) in positions {
        let Some((last_date, last_id)) = repo::latest_event(conn, account, security, this)? else {
            continue;
        };
        let later = match this {
            Some(id) => (last_date, last_id) > (date, id),
            None => last_date > date,
        };
        if later {
            let sec = securities::get(conn, security)?;
            let acct = accounts::get(conn, account)?;
            return Err(Error::Invalid(format!(
                "{} in {:?} has a sale, transfer, split, or return of capital on {last_date}; \
                 change or delete that first (a holding's history is kept in date order)",
                sec.label(),
                acct.fields.name
            )));
        }
    }
    Ok(())
}

fn check_settle(input: &InvInput) -> Result<()> {
    match input.settle_date {
        Some(s) if s < input.date => Err(Error::Invalid(
            "the settlement date is before the trade date".into(),
        )),
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

/// Which fields an action uses; anything else must be empty.
fn check_fields(i: &InvInput) -> Result<()> {
    let a = i.action;
    let name = a.label();
    let unused = |what: &str| Err(Error::Invalid(format!("{name} takes no {what}")));
    if a.needs_security() && i.security.is_none() {
        return Err(Error::Invalid(format!("{name} needs a security")));
    }
    if !a.allows_security() && i.security.is_some() {
        return unused("security");
    }
    if a.takes_quantity() {
        match i.quantity {
            None => return Err(Error::Invalid(format!("{name} needs a number of shares"))),
            Some(q) if q.raw() <= 0 => {
                return Err(Error::Invalid("shares must be more than zero".into()));
            }
            Some(_) => {}
        }
    } else if i.quantity.is_some() {
        return unused("shares");
    }
    let priced = matches!(
        a,
        InvAction::Buy
            | InvAction::Sell
            | InvAction::ReinvestDividend
            | InvAction::ReinvestCgShort
            | InvAction::ReinvestCgLong
            | InvAction::SharesAdded
            | InvAction::SharesRemoved
            | InvAction::TransferShares
    );
    if !priced && i.price.is_some() {
        return unused("price");
    }
    if i.price.is_some_and(|p| p.is_negative()) {
        return Err(Error::Invalid("a price cannot be negative".into()));
    }
    if i.commission.is_negative() {
        return Err(Error::Invalid("commission cannot be negative".into()));
    }
    if !matches!(a, InvAction::Buy | InvAction::Sell) && !i.commission.is_zero() {
        return unused("commission");
    }
    if !a.takes_amount() && i.amount.is_some() {
        return unused("amount");
    }
    if i.amount.is_some_and(|m| m.is_negative()) {
        return Err(Error::Invalid(
            "enter the amount as a positive number; the action gives its direction".into(),
        ));
    }
    if (a == InvAction::Split) != i.split.is_some() {
        return if a == InvAction::Split {
            Err(Error::Invalid("a split needs its ratio".into()))
        } else {
            unused("split ratio")
        };
    }
    if (a == InvAction::TransferShares) != i.to_account.is_some() {
        return if a == InvAction::TransferShares {
            Err(Error::Invalid(
                "choose the account receiving the shares".into(),
            ))
        } else {
            unused("receiving account")
        };
    }
    if !a.disposes() && (i.lot_method.is_some() || !i.lots.is_empty()) {
        return unused("lot selection");
    }
    if a != InvAction::SharesAdded && i.acquired.is_some() {
        return unused("acquisition date");
    }
    let counterpart_ok = match a {
        InvAction::CashIn | InvAction::CashOut => true,
        InvAction::MiscIncome | InvAction::MiscExpense => {
            matches!(i.counterpart, None | Some(Target::Category(_)))
        }
        _ => i.counterpart.is_none(),
    };
    if !counterpart_ok {
        return unused("transfer account or category");
    }
    check_settle(i)
}

/// Collects postings and lot records.
struct Builder {
    postings: Vec<PostingInput>,
    lots: Vec<PlannedLot>,
    disposals: Vec<PlannedDisposal>,
    adjustments: Vec<Adjustment>,
}

impl Builder {
    fn post(&mut self, target: Target, security: Option<SecurityId>, amount: Money) {
        let mut p = PostingInput::new(target, amount);
        p.security = security;
        self.postings.push(p);
    }

    fn category(&mut self, conn: &Connection, which: SystemCategory, amount: Money) -> Result<()> {
        let id = categories::system(conn, which)?.id;
        self.post(Target::Category(id), None, amount);
        Ok(())
    }
}

fn neg(m: Money) -> Result<Money> {
    m.checked_neg().ok_or(Error::Overflow("investment posting"))
}

fn sub(a: Money, b: Money) -> Result<Money> {
    a.checked_sub(b)
        .ok_or(Error::Overflow("investment posting"))
}

/// Work out and check everything `input` would write. When editing,
/// `editing` is the stored transaction, whose lot records have already
/// been removed.
pub(crate) fn plan(conn: &Connection, input: &InvInput, editing: Option<&InvTxn>) -> Result<Plan> {
    let acct = investment_account(conn, input.account)?;
    if acct.status == AccountStatus::Closed {
        return Err(Error::Invalid(format!(
            "account {:?} is closed",
            acct.fields.name
        )));
    }
    let settings = acct.fields.investment.clone().ok_or_else(|| {
        Error::Invalid(format!("{:?} has no investment settings", acct.fields.name))
    })?;
    check_fields(input)?;
    let action = input.action;
    let a = input.account;

    let security: Option<Security> = input
        .security
        .map(|id| securities::get(conn, id))
        .transpose()?;
    if let Some(s) = &security {
        if s.fields.security_type == SecurityType::MoneyMarket
            && settings.mmf_mode == MmfMode::Cash
            && action.takes_quantity()
        {
            return Err(Error::Invalid(format!(
                "{:?} keeps money market funds as cash; record {} as cash, or change the \
                 account to hold money market funds as securities",
                acct.fields.name,
                s.label()
            )));
        }
    }

    // Where cash goes (INV-300).
    let cash_account = match settings.cash_mode {
        CashMode::Internal => a,
        CashMode::Linked => {
            let linked = settings.linked_cash_account.ok_or_else(|| {
                Error::Invalid(format!("{:?} has no linked cash account", acct.fields.name))
            })?;
            let l = accounts::get(conn, linked)?;
            if action.cash_direction() != 0 && l.status == AccountStatus::Closed {
                return Err(Error::Invalid(format!(
                    "linked cash account {:?} is closed",
                    l.fields.name
                )));
            }
            if matches!(action, InvAction::CashIn | InvAction::CashOut) {
                return Err(Error::Invalid(format!(
                    "{:?} keeps its cash in {:?}; transfer money to or from that account",
                    acct.fields.name, l.fields.name
                )));
            }
            linked
        }
    };

    let to = match input.to_account {
        Some(to) => {
            if to == a {
                return Err(Error::Invalid(
                    "shares must go to a different account".into(),
                ));
            }
            let dest = investment_account(conn, to)?;
            if dest.status == AccountStatus::Closed {
                return Err(Error::Invalid(format!(
                    "account {:?} is closed",
                    dest.fields.name
                )));
            }
            let dest_mmf = dest.fields.investment.as_ref().map(|i| i.mmf_mode);
            if let Some(s) = &security {
                if s.fields.security_type == SecurityType::MoneyMarket
                    && dest_mmf == Some(MmfMode::Cash)
                {
                    return Err(Error::Invalid(format!(
                        "{:?} keeps money market funds as cash",
                        dest.fields.name
                    )));
                }
            }
            Some(to)
        }
        None => None,
    };

    let this = editing.map(|t| t.txn.id);
    if action.affects_lots() {
        if let Some(s) = &security {
            let mut held = vec![(a, s.id)];
            if let Some(to) = to {
                held.push((to, s.id));
            }
            check_order(conn, &held, input.date, this)?;
        }
    }

    // Lots of this holding open on the trade date.
    let open = |s: &Security| -> Result<Vec<OpenLot>> {
        Ok(repo::open_lots(conn, Some(a), Some(s.id), input.date)?
            .into_iter()
            .map(|l| l.open_lot())
            .collect())
    };

    let amount = resolve_amount(input)?;
    let mut b = Builder {
        postings: Vec::new(),
        lots: Vec::new(),
        disposals: Vec::new(),
        adjustments: Vec::new(),
    };
    let mut lot_method = None;
    let cash = Target::Account(cash_account);
    let qty = input.quantity.unwrap_or(Quantity::ZERO);

    match action {
        InvAction::Buy
        | InvAction::ReinvestDividend
        | InvAction::ReinvestCgShort
        | InvAction::ReinvestCgLong
        | InvAction::SharesAdded => {
            let s = security
                .as_ref()
                .ok_or(Error::Invalid("no security".into()))?;
            let acquired = input.acquired.unwrap_or(input.date);
            if acquired > input.date {
                return Err(Error::Invalid(
                    "the acquisition date is after the transaction date".into(),
                ));
            }
            match action {
                InvAction::Buy => b.post(cash, None, neg(amount)?),
                InvAction::SharesAdded => {}
                _ if amount.is_zero() => {
                    return Err(Error::Invalid("the amount must be more than zero".into()));
                }
                _ => {}
            }
            b.post(Target::Account(a), Some(s.id), amount);
            match action.category() {
                Some(c) => b.category(conn, c, neg(amount)?)?,
                None if action == InvAction::SharesAdded => {
                    b.category(conn, SystemCategory::OpeningBalance, neg(amount)?)?
                }
                None => {}
            }
            b.lots.push(PlannedLot {
                account: a,
                security: s.id,
                acquired,
                quantity: qty,
                basis: amount,
                source: None,
            });
        }
        InvAction::Sell | InvAction::TransferShares | InvAction::SharesRemoved => {
            let s = security
                .as_ref()
                .ok_or(Error::Invalid("no security".into()))?;
            let (method, takes) = choose_lots(input, s, &settings.default_lot_method, &open(s)?)?;
            lot_method = Some(method);
            let basis: Money = takes.iter().map(|t| t.basis).sum();
            match action {
                InvAction::Sell => {
                    let proceeds = lots::divide_proceeds(&takes, amount)?;
                    for (t, p) in takes.iter().zip(&proceeds) {
                        b.disposals.push(PlannedDisposal {
                            lot: t.lot,
                            kind: DisposalKind::Sale,
                            quantity: t.quantity,
                            basis: t.basis,
                            proceeds: Some(*p),
                            term: Some(lots::term(t.acquired, input.date)),
                        });
                    }
                    b.post(cash, None, amount);
                    b.post(Target::Account(a), Some(s.id), neg(basis)?);
                    let gain = sub(amount, basis)?;
                    if !gain.is_zero() {
                        b.category(conn, SystemCategory::RealizedGain, neg(gain)?)?;
                    }
                }
                InvAction::TransferShares => {
                    let dest = to.ok_or(Error::Invalid("no receiving account".into()))?;
                    for t in &takes {
                        b.disposals.push(disposal(t, DisposalKind::TransferOut));
                        b.lots.push(PlannedLot {
                            account: dest,
                            security: s.id,
                            acquired: t.acquired,
                            quantity: t.quantity,
                            basis: t.basis,
                            source: Some(t.lot),
                        });
                    }
                    b.post(Target::Account(a), Some(s.id), neg(basis)?);
                    b.post(Target::Account(dest), Some(s.id), basis);
                }
                _ => {
                    for t in &takes {
                        b.disposals.push(disposal(t, DisposalKind::Removed));
                    }
                    b.post(Target::Account(a), Some(s.id), neg(basis)?);
                    b.category(conn, SystemCategory::OpeningBalance, basis)?;
                }
            }
        }
        InvAction::Split => {
            let s = security
                .as_ref()
                .ok_or(Error::Invalid("no security".into()))?;
            let ratio = input
                .split
                .ok_or(Error::Invalid("a split needs its ratio".into()))?;
            for (lot, delta) in lots::split(&open(s)?, ratio.new, ratio.old)? {
                if !delta.is_zero() {
                    b.adjustments.push(Adjustment {
                        lot,
                        kind: AdjustmentKind::Split,
                        quantity_delta: delta,
                        basis_delta: Money::ZERO,
                    });
                }
            }
            b.post(Target::Account(a), Some(s.id), Money::ZERO);
        }
        InvAction::ReturnOfCapital => {
            let s = security
                .as_ref()
                .ok_or(Error::Invalid("no security".into()))?;
            let (cuts, excess) = lots::return_of_capital(&open(s)?, amount)?;
            let mut reduced = Money::ZERO;
            for (lot, cut) in cuts {
                if cut.cents() > 0 {
                    reduced += cut;
                    b.adjustments.push(Adjustment {
                        lot,
                        kind: AdjustmentKind::ReturnOfCapital,
                        quantity_delta: Quantity::ZERO,
                        basis_delta: neg(cut)?,
                    });
                }
            }
            b.post(cash, None, amount);
            b.post(Target::Account(a), Some(s.id), neg(reduced)?);
            if !excess.is_zero() {
                b.category(conn, SystemCategory::RealizedGain, neg(excess)?)?;
            }
        }
        InvAction::CashIn | InvAction::CashOut => {
            let other = input.counterpart.ok_or_else(|| {
                Error::Invalid(
                    "choose the account or category the cash comes from or goes to".into(),
                )
            })?;
            check_counterpart(conn, other, a)?;
            let signed = if action == InvAction::CashIn {
                amount
            } else {
                neg(amount)?
            };
            b.post(cash, None, signed);
            b.post(other, None, neg(signed)?);
        }
        InvAction::Dividend
        | InvAction::Interest
        | InvAction::CgDistShort
        | InvAction::CgDistLong
        | InvAction::Fee
        | InvAction::TaxWithholding
        | InvAction::MiscIncome
        | InvAction::MiscExpense => {
            let signed = if action.cash_direction() > 0 {
                amount
            } else {
                neg(amount)?
            };
            b.post(cash, None, signed);
            match input.counterpart {
                Some(Target::Category(c)) => {
                    categories::get(conn, c)?;
                    b.post(Target::Category(c), None, neg(signed)?);
                }
                _ => {
                    let c = action
                        .category()
                        .ok_or(Error::Invalid("no category for this action".into()))?;
                    b.category(conn, c, neg(signed)?)?;
                }
            }
        }
    }

    let mut stored = input.clone();
    if action.disposes() {
        stored.lot_method = lot_method;
    }
    Ok(Plan {
        txn: TxnInput {
            date: input.date,
            payee: None,
            check_num: String::new(),
            memo: input.memo.clone(),
            notes: String::new(),
            postings: b.postings,
        },
        input: stored,
        lot_method,
        lots: b.lots,
        disposals: b.disposals,
        adjustments: b.adjustments,
    })
}

fn disposal(t: &Take, kind: DisposalKind) -> PlannedDisposal {
    PlannedDisposal {
        lot: t.lot,
        kind,
        quantity: t.quantity,
        basis: t.basis,
        proceeds: None,
        term: None,
    }
}

/// The action's amount: as given, or shares × price for trades.
fn resolve_amount(i: &InvInput) -> Result<Money> {
    if !i.action.takes_amount() {
        return Ok(Money::ZERO);
    }
    let amount = match (i.amount, i.quantity, i.price) {
        (Some(m), _, _) => m,
        (None, Some(q), Some(p))
            if matches!(
                i.action,
                InvAction::Buy
                    | InvAction::Sell
                    | InvAction::ReinvestDividend
                    | InvAction::ReinvestCgShort
                    | InvAction::ReinvestCgLong
            ) =>
        {
            trade_amount(i.action, q, p, i.commission)?
        }
        _ if i.action.takes_quantity() && i.action != InvAction::SharesAdded => {
            return Err(Error::Invalid(format!(
                "{} needs a price or an amount",
                i.action.label()
            )));
        }
        _ if i.action == InvAction::SharesAdded => {
            return Err(Error::Invalid("shares added need their cost basis".into()));
        }
        _ => {
            return Err(Error::Invalid(format!(
                "{} needs an amount",
                i.action.label()
            )));
        }
    };
    if amount.is_negative() {
        return Err(Error::Invalid(
            "the commission is more than the sale is worth".into(),
        ));
    }
    // Trades and shares added may be worth nothing (a gift, a worthless
    // sale); reinvestments are checked where they are planned.
    if amount.is_zero() && !i.action.takes_quantity() {
        return Err(Error::Invalid("the amount must be more than zero".into()));
    }
    Ok(amount)
}

/// Resolve the lot selection method (LOT-100: the sale's own, else the
/// security's, else the account's) and choose the lots.
fn choose_lots(
    input: &InvInput,
    s: &Security,
    account_default: &LotMethod,
    open: &[OpenLot],
) -> Result<(LotMethod, Vec<Take>)> {
    let method = if input.lots.is_empty() {
        input
            .lot_method
            .or(s.fields.default_lot_method)
            .unwrap_or(*account_default)
    } else {
        match input.lot_method {
            None | Some(LotMethod::Specific) => LotMethod::Specific,
            Some(m) => {
                return Err(Error::Invalid(format!(
                    "lots are chosen by hand only with specific identification, not {m}"
                )));
            }
        }
    };
    let quantity = input
        .quantity
        .ok_or(Error::Invalid("no number of shares".into()))?;
    let takes = match method {
        LotMethod::Fifo => lots::pick_fifo(open, quantity)?,
        LotMethod::Specific => lots::pick_specific(open, &input.lots, quantity)?,
        other => {
            return Err(Error::Invalid(format!(
                "{other} lot selection is not available yet; choose fifo or specific lots"
            )));
        }
    };
    Ok((method, takes))
}

/// Cash in or out: another open account that is not an investment
/// account, or a category.
fn check_counterpart(conn: &Connection, target: Target, this: AccountId) -> Result<()> {
    match target {
        Target::Account(other) => {
            if other == this {
                return Err(Error::Invalid(
                    "cash cannot move from an account to itself".into(),
                ));
            }
            let acct = accounts::get(conn, other)?;
            if acct.fields.account_type.is_investment() {
                return Err(Error::Invalid(format!(
                    "{:?} is an investment account; move cash out of one and into the other",
                    acct.fields.name
                )));
            }
            if acct.status == AccountStatus::Closed {
                return Err(Error::Invalid(format!(
                    "account {:?} is closed",
                    acct.fields.name
                )));
            }
        }
        Target::Category(c) => {
            categories::get(conn, c)?;
        }
    }
    Ok(())
}
