//! Lot arithmetic (LOT-020 … LOT-130): which lots a disposal takes, how
//! basis and proceeds divide, splits, return of capital, holding period.
//! Pure functions over open-lot snapshots; no SQL.

use super::{LotId, LotPick, Term};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::{Money, Quantity, allocate, mul_div};

/// A lot's state just before the transaction being planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OpenLot {
    pub id: LotId,
    pub acquired: Date,
    pub quantity: Quantity,
    pub basis: Money,
}

/// Part of one lot leaving it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Take {
    pub lot: LotId,
    pub acquired: Date,
    pub quantity: Quantity,
    pub basis: Money,
}

/// Basis of `quantity` shares out of a lot holding `open` shares with
/// `basis`: proportional, rounded half-even; all of it when the lot
/// empties, so nothing is left behind (LOT-030).
fn basis_of(open: Quantity, basis: Money, quantity: Quantity) -> Result<Money> {
    if quantity == open {
        return Ok(basis);
    }
    mul_div(basis.cents(), quantity.raw(), open.raw()).map(Money::from_cents)
}

fn take(lot: &OpenLot, quantity: Quantity) -> Result<Take> {
    Ok(Take {
        lot: lot.id,
        acquired: lot.acquired,
        quantity,
        basis: basis_of(lot.quantity, lot.basis, quantity)?,
    })
}

fn total(lots: &[OpenLot]) -> Quantity {
    lots.iter().map(|l| l.quantity).sum()
}

fn short_of(have: Quantity, want: Quantity) -> Error {
    Error::Invalid(format!("only {have} shares are held; cannot take {want}"))
}

/// First in, first out: oldest acquisition date first, then entry order.
pub(crate) fn pick_fifo(lots: &[OpenLot], quantity: Quantity) -> Result<Vec<Take>> {
    let have = total(lots);
    if quantity > have {
        return Err(short_of(have, quantity));
    }
    let mut ordered: Vec<&OpenLot> = lots.iter().collect();
    ordered.sort_by_key(|l| (l.acquired, l.id));
    let mut left = quantity;
    let mut out = Vec::new();
    for lot in ordered {
        if left.is_zero() {
            break;
        }
        let q = left.min(lot.quantity);
        out.push(take(lot, q)?);
        left -= q;
    }
    Ok(out)
}

/// Specific identification: the user's picks, which must add up to
/// `quantity`, name each open lot once, and not exceed what it holds.
pub(crate) fn pick_specific(
    lots: &[OpenLot],
    picks: &[LotPick],
    quantity: Quantity,
) -> Result<Vec<Take>> {
    if picks.is_empty() {
        return Err(Error::Invalid("choose the lots to take shares from".into()));
    }
    let mut out = Vec::with_capacity(picks.len());
    let mut sum = Quantity::ZERO;
    for (i, pick) in picks.iter().enumerate() {
        if picks[..i].iter().any(|p| p.lot == pick.lot) {
            return Err(Error::Invalid(format!(
                "lot {} is chosen twice",
                pick.lot.0
            )));
        }
        if pick.quantity.raw() <= 0 {
            return Err(Error::Invalid(format!(
                "lot {}: shares must be more than zero",
                pick.lot.0
            )));
        }
        let lot = lots.iter().find(|l| l.id == pick.lot).ok_or_else(|| {
            Error::Invalid(format!(
                "lot {} is not an open lot of this holding on this date",
                pick.lot.0
            ))
        })?;
        if pick.quantity > lot.quantity {
            return Err(Error::Invalid(format!(
                "lot {} holds {} shares; cannot take {}",
                lot.id.0, lot.quantity, pick.quantity
            )));
        }
        sum = sum
            .checked_add(pick.quantity)
            .ok_or(Error::Overflow("lot picks"))?;
        out.push(take(lot, pick.quantity)?);
    }
    if sum != quantity {
        return Err(Error::Invalid(format!(
            "the chosen lots add up to {sum} shares, not {quantity}"
        )));
    }
    Ok(out)
}

/// Divide sale proceeds among the lots sold, by shares (LOT-030, LOT-040).
pub(crate) fn divide_proceeds(takes: &[Take], proceeds: Money) -> Result<Vec<Money>> {
    let weights: Vec<i64> = takes.iter().map(|t| t.quantity.raw()).collect();
    Ok(allocate(proceeds.cents(), &weights)?
        .into_iter()
        .map(Money::from_cents)
        .collect())
}

/// Short-term if held one year or less, long-term otherwise (LOT-040):
/// long-term once the sale date is after the acquisition's anniversary
/// (for a 29 February acquisition, after 28 February next year).
pub(crate) fn term(acquired: Date, sold: Date) -> Term {
    let anniversary = acquired
        .naive()
        .checked_add_months(chrono::Months::new(12))
        .map(Date::from_naive);
    match anniversary {
        Some(a) if sold > a => Term::Long,
        Some(_) => Term::Short,
        // Beyond the calendar's range: far in the future, so held long.
        None => Term::Long,
    }
}

/// Split `new` for `old` (LOT-120): the position's new share count is
/// rounded half-even to 6 places once, then divided among the lots by
/// their shares, so rounding never makes or loses shares beyond that one
/// rounding. Returns each lot's change in shares; basis and dates stay.
pub(crate) fn split(lots: &[OpenLot], new: i64, old: i64) -> Result<Vec<(LotId, Quantity)>> {
    if new <= 0 || old <= 0 || new == old {
        return Err(Error::Invalid(
            "a split needs two different positive numbers of shares".into(),
        ));
    }
    let held = total(lots);
    if held.is_zero() {
        return Err(Error::Invalid("no shares are held to split".into()));
    }
    let after = mul_div(held.raw(), new, old)?;
    let weights: Vec<i64> = lots.iter().map(|l| l.quantity.raw()).collect();
    let parts = allocate(after, &weights)?;
    let mut out = Vec::with_capacity(lots.len());
    for (lot, part) in lots.iter().zip(parts) {
        if part == 0 {
            return Err(Error::Invalid(format!(
                "the split would leave lot {} with no shares",
                lot.id.0
            )));
        }
        out.push((lot.id, Quantity::from_raw(part) - lot.quantity));
    }
    Ok(out)
}

/// Return of capital (LOT-130): `amount` divided among the open lots by
/// shares; each lot's basis goes down by its part, but not below zero.
/// Returns each lot's reduction and the excess over basis, which is a
/// capital gain.
pub(crate) fn return_of_capital(
    lots: &[OpenLot],
    amount: Money,
) -> Result<(Vec<(LotId, Money)>, Money)> {
    if total(lots).is_zero() {
        return Err(Error::Invalid(
            "no shares are held to return capital on".into(),
        ));
    }
    let weights: Vec<i64> = lots.iter().map(|l| l.quantity.raw()).collect();
    let parts = allocate(amount.cents(), &weights)?;
    let mut reductions = Vec::with_capacity(lots.len());
    let mut excess = Money::ZERO;
    for (lot, part) in lots.iter().zip(parts) {
        let part = Money::from_cents(part);
        let cut = part.min(lot.basis);
        reductions.push((lot.id, cut));
        excess += part - cut;
    }
    Ok((reductions, excess))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }
    fn q(s: &str) -> Quantity {
        s.parse().unwrap()
    }
    fn m(s: &str) -> Money {
        s.parse().unwrap()
    }
    fn lot(id: i64, acquired: &str, quantity: &str, basis: &str) -> OpenLot {
        OpenLot {
            id: LotId(id),
            acquired: d(acquired),
            quantity: q(quantity),
            basis: m(basis),
        }
    }

    #[test]
    fn fifo_takes_oldest_first_by_acquisition_date() {
        // Lot 2 was entered later but acquired earlier (a transfer in).
        let lots = [
            lot(1, "2025-03-01", "10", "1000.00"),
            lot(2, "2024-06-01", "5", "400.00"),
        ];
        let t = pick_fifo(&lots, q("8")).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(
            (t[0].lot, t[0].quantity, t[0].basis),
            (LotId(2), q("5"), m("400.00"))
        );
        assert_eq!(
            (t[1].lot, t[1].quantity, t[1].basis),
            (LotId(1), q("3"), m("300.00"))
        );
        assert!(pick_fifo(&lots, q("15.000001")).is_err());
    }

    #[test]
    fn partial_basis_rounds_and_the_last_share_takes_the_rest() {
        let lots = [lot(1, "2025-01-01", "3", "100.00")];
        let a = pick_fifo(&lots, q("1")).unwrap();
        assert_eq!(a[0].basis, m("33.33"));
        let rest = [lot(1, "2025-01-01", "2", "66.67")];
        assert_eq!(pick_fifo(&rest, q("2")).unwrap()[0].basis, m("66.67"));
    }

    #[test]
    fn specific_picks_are_checked() {
        let lots = [
            lot(1, "2025-01-01", "10", "100.00"),
            lot(2, "2025-02-01", "10", "200.00"),
        ];
        let pick = |id, s: &str| LotPick {
            lot: LotId(id),
            quantity: q(s),
        };
        let t = pick_specific(&lots, &[pick(2, "4")], q("4")).unwrap();
        assert_eq!((t[0].lot, t[0].basis), (LotId(2), m("80.00")));
        for (picks, want, msg) in [
            (vec![], "1", "choose the lots"),
            (vec![pick(1, "1"), pick(1, "1")], "2", "chosen twice"),
            (vec![pick(3, "1")], "1", "not an open lot"),
            (vec![pick(1, "11")], "11", "holds 10 shares"),
            (vec![pick(1, "1")], "2", "add up to 1 shares, not 2"),
            (vec![pick(1, "0")], "0", "more than zero"),
        ] {
            let err = pick_specific(&lots, &picks, q(want))
                .unwrap_err()
                .to_string();
            assert!(err.contains(msg), "{err}");
        }
    }

    #[test]
    fn proceeds_divide_exactly() {
        let takes = [
            Take {
                lot: LotId(1),
                acquired: d("2025-01-01"),
                quantity: q("1"),
                basis: Money::ZERO,
            },
            Take {
                lot: LotId(2),
                acquired: d("2025-01-01"),
                quantity: q("2"),
                basis: Money::ZERO,
            },
        ];
        assert_eq!(
            divide_proceeds(&takes, m("100.00")).unwrap(),
            vec![m("33.33"), m("66.67")]
        );
    }

    #[test]
    fn holding_period_boundary() {
        assert_eq!(term(d("2025-01-15"), d("2026-01-15")), Term::Short);
        assert_eq!(term(d("2025-01-15"), d("2026-01-16")), Term::Long);
        assert_eq!(term(d("2024-02-29"), d("2025-02-28")), Term::Short);
        assert_eq!(term(d("2024-02-29"), d("2025-03-01")), Term::Long);
    }

    #[test]
    fn split_keeps_total_rounded_once() {
        let lots = [
            lot(1, "2025-01-01", "1", "10.00"),
            lot(2, "2025-01-01", "1", "10.00"),
            lot(3, "2025-01-01", "1", "10.00"),
        ];
        // 3 shares, 1-for-3 reverse split: exactly 1 share in total.
        let deltas = split(&lots, 1, 3).unwrap();
        let after: Vec<i64> = lots
            .iter()
            .zip(&deltas)
            .map(|(l, (_, dq))| (l.quantity + *dq).raw())
            .collect();
        assert_eq!(after, vec![333_334, 333_333, 333_333]);
        assert_eq!(after.iter().sum::<i64>(), 1_000_000);
        assert!(split(&lots, 2, 2).is_err());
        let tiny = [
            lot(1, "2025-01-01", "0.000001", "1.00"),
            lot(2, "2025-01-01", "5", "1.00"),
        ];
        assert!(
            split(&tiny, 1, 10)
                .unwrap_err()
                .to_string()
                .contains("no shares")
        );
    }

    #[test]
    fn return_of_capital_stops_at_zero_basis() {
        let lots = [
            lot(1, "2025-01-01", "10", "50.00"),
            lot(2, "2025-01-01", "10", "500.00"),
        ];
        let (cuts, excess) = return_of_capital(&lots, m("200.00")).unwrap();
        assert_eq!(cuts, vec![(LotId(1), m("50.00")), (LotId(2), m("100.00"))]);
        assert_eq!(excess, m("50.00"));
    }
}
