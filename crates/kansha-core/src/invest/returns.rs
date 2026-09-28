//! Return measures (POS-030): internal rate of return and time-weighted
//! return. Pure arithmetic in `rust_decimal`; no floats.
//!
//! IRR is money-weighted and annual: the yearly rate at which the money
//! put in (negative, from the investor's side) and taken out (positive,
//! the ending value included) are worth nothing today, with a year of
//! 365 days. It is solved on a daily rate by bisection using whole-number
//! powers only, so no logarithms are needed.
//!
//! The time-weighted return is for the whole period, not annual: the
//! value's growth between one flow and the next, chained, so money coming
//! and going does not count as gain. A flow is taken at the end of its
//! day: the value that day, less the flow, closes the stretch before it.

use std::cmp::Ordering;

use rust_decimal::{Decimal, RoundingStrategy};

use crate::date::Date;
use crate::money::Money;

/// Daily-rate search bounds: about −99.99 % and +10 000 % a year.
const DAILY_LOW: Decimal = Decimal::from_parts(249, 0, 0, true, 4); // -0.0249
const DAILY_HIGH: Decimal = Decimal::from_parts(127, 0, 0, false, 4); // 0.0127

/// `base` to a whole power, by squaring; `None` on overflow.
fn pow(base: Decimal, mut n: u64) -> Option<Decimal> {
    let mut result = Decimal::ONE;
    let mut b = base;
    while n > 0 {
        if n & 1 == 1 {
            result = result.checked_mul(b)?;
        }
        n >>= 1;
        if n > 0 {
            b = b.checked_mul(b)?;
        }
    }
    Some(result)
}

/// The sign of the flows' net value at daily rate `d`. Every factor is
/// kept at 1 or below, so nothing overflows: discounted to the first day
/// when `d >= 0`, carried to the last day otherwise (the two differ by a
/// positive factor, so their sign agrees).
fn npv_sign(flows: &[(i64, Decimal)], d: Decimal) -> Option<Ordering> {
    let growth = Decimal::ONE.checked_add(d)?;
    let last = flows.iter().map(|(t, _)| *t).max().unwrap_or(0);
    let mut sum = Decimal::ZERO;
    for (t, cf) in flows {
        let factor = if growth >= Decimal::ONE {
            pow(Decimal::ONE.checked_div(growth)?, u64::try_from(*t).ok()?)?
        } else {
            pow(growth, u64::try_from(last - t).ok()?)?
        };
        sum = sum.checked_add(cf.checked_mul(factor)?)?;
    }
    Some(sum.cmp(&Decimal::ZERO))
}

/// Annual internal rate of return of dated flows, investor's side:
/// negative = money put in, positive = money taken out. `None` without
/// both kinds of flow, or when no rate between about −99.99 % and
/// +10 000 % a year fits (a total loss, for one).
pub fn irr(flows: &[(Date, Money)]) -> Option<Decimal> {
    let first = flows.iter().map(|(d, _)| *d).min()?;
    let points: Vec<(i64, Decimal)> = flows
        .iter()
        .filter(|(_, m)| !m.is_zero())
        .map(|(d, m)| {
            let days = d.naive().signed_duration_since(first.naive()).num_days();
            (days, m.to_decimal())
        })
        .collect();
    let has = |o: Ordering| points.iter().any(|(_, m)| m.cmp(&Decimal::ZERO) == o);
    if !has(Ordering::Less) || !has(Ordering::Greater) {
        return None;
    }
    let (mut lo, mut hi) = (DAILY_LOW, DAILY_HIGH);
    let s_lo = npv_sign(&points, lo)?;
    let s_hi = npv_sign(&points, hi)?;
    let daily = if s_lo == Ordering::Equal {
        lo
    } else if s_hi == Ordering::Equal {
        hi
    } else if s_lo == s_hi {
        return None;
    } else {
        let two = Decimal::TWO;
        let mut mid = lo;
        for _ in 0..200 {
            mid = (lo.checked_add(hi)?).checked_div(two)?;
            if mid == lo || mid == hi {
                break;
            }
            match npv_sign(&points, mid)? {
                Ordering::Equal => break,
                s if s == s_lo => lo = mid,
                _ => hi = mid,
            }
        }
        mid
    };
    pow(Decimal::ONE.checked_add(daily)?, 365)?.checked_sub(Decimal::ONE)
}

/// Time-weighted return for the whole period. `start` is the value just
/// before it; `points` the value at the end of each flow day with that
/// day's flow (money in positive), in date order; `end` the value at its
/// end. Stretches that start at zero and stay there are skipped; `None`
/// when a stretch starts at zero or below and moves, or none can be
/// measured.
pub fn twr(start: Money, points: &[(Money, Money)], end: Money) -> Option<Decimal> {
    let mut product = Decimal::ONE;
    let mut measured = false;
    let mut prev = start.to_decimal();
    let mut stretch = |from: Decimal, to: Decimal| -> Option<()> {
        if from.is_zero() {
            return if to.is_zero() { Some(()) } else { None };
        }
        if from < Decimal::ZERO {
            return None;
        }
        product = product.checked_mul(to.checked_div(from)?)?;
        measured = true;
        Some(())
    };
    for (value, flow) in points {
        let before = value.to_decimal().checked_sub(flow.to_decimal())?;
        stretch(prev, before)?;
        prev = value.to_decimal();
    }
    stretch(prev, end.to_decimal())?;
    measured.then(|| product - Decimal::ONE)
}

/// A rate as a percent with two decimals, half-even; never "-0.00".
pub fn percent_text(rate: Decimal) -> Option<String> {
    let p = rate
        .checked_mul(Decimal::ONE_HUNDRED)?
        .round_dp_with_strategy(2, RoundingStrategy::MidpointNearestEven);
    Some(format!(
        "{:.2}",
        if p.is_zero() { Decimal::ZERO } else { p }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }
    fn m(s: &str) -> Money {
        s.parse().unwrap()
    }
    fn pct(r: Option<Decimal>) -> Option<String> {
        r.and_then(percent_text)
    }
    fn irr_of(flows: &[(&str, &str)]) -> Option<String> {
        let f: Vec<(Date, Money)> = flows.iter().map(|(a, b)| (d(a), m(b))).collect();
        pct(irr(&f))
    }

    #[test]
    fn irr_of_one_year_growing_ten_percent() {
        assert_eq!(
            irr_of(&[("2025-01-01", "-1000.00"), ("2026-01-01", "1100.00")]).as_deref(),
            Some("10.00")
        );
    }

    #[test]
    fn irr_counts_days_in_365_day_years() {
        // 366 days in 2024: 1.1^(365/366) - 1 = 9.97 %.
        assert_eq!(
            irr_of(&[("2024-01-01", "-1000.00"), ("2025-01-01", "1100.00")]).as_deref(),
            Some("9.97")
        );
        // 90 days at 1 %: 1.01^(365/90) - 1 = 4.12 % a year.
        assert_eq!(
            irr_of(&[("2026-01-01", "-1000.00"), ("2026-04-01", "1010.00")]).as_deref(),
            Some("4.12")
        );
    }

    #[test]
    fn irr_with_two_deposits_is_the_rate_that_explains_the_end() {
        // 1000 x 1.1^2 + 1000 x 1.1 = 2310.
        assert_eq!(
            irr_of(&[
                ("2025-01-01", "-1000.00"),
                ("2026-01-01", "-1000.00"),
                ("2027-01-01", "2310.00"),
            ])
            .as_deref(),
            Some("10.00")
        );
        // A withdrawal in between: 1000 grows 10 % a year, 550 comes out
        // after a year, 605 is left a year later.
        assert_eq!(
            irr_of(&[
                ("2025-01-01", "-1000.00"),
                ("2026-01-01", "550.00"),
                ("2027-01-01", "605.00"),
            ])
            .as_deref(),
            Some("10.00")
        );
    }

    #[test]
    fn irr_of_a_loss_no_change_and_the_cases_without_an_answer() {
        assert_eq!(
            irr_of(&[("2025-01-01", "-1000.00"), ("2026-01-01", "800.00")]).as_deref(),
            Some("-20.00")
        );
        assert_eq!(
            irr_of(&[("2025-01-01", "-1000.00"), ("2026-01-01", "1000.00")]).as_deref(),
            Some("0.00")
        );
        // Everything lost: -100 % is outside the search.
        assert_eq!(
            irr_of(&[("2025-01-01", "-1000.00"), ("2026-01-01", "0.00")]),
            None
        );
        // Only money in, or nothing at all.
        assert_eq!(irr_of(&[("2025-01-01", "-1000.00")]), None);
        assert_eq!(irr_of(&[]), None);
        // Over twenty years, still no overflow: 7 % a year.
        let end = "3869.68"; // 1000 x 1.07^20
        assert_eq!(
            irr_of(&[("2000-01-01", "-1000.00"), ("2019-12-27", end)]).as_deref(),
            Some("7.00")
        );
    }

    #[test]
    fn twr_chains_growth_between_flows() {
        // 1000 grows to 1100 (+10 %); 500 comes in, 1600 grows to 1760
        // (+10 %): 1.1 x 1.1 - 1 = 21 %, whatever the deposit.
        let r = twr(m("1000.00"), &[(m("1600.00"), m("500.00"))], m("1760.00"));
        assert_eq!(pct(r).as_deref(), Some("21.00"));
        // No flows: simple growth.
        assert_eq!(
            pct(twr(m("1000.00"), &[], m("1050.00"))).as_deref(),
            Some("5.00")
        );
    }

    #[test]
    fn twr_starting_from_nothing_measures_from_the_first_deposit() {
        // Opened in the period: 1000 in, grows to 1200.
        let r = twr(Money::ZERO, &[(m("1000.00"), m("1000.00"))], m("1200.00"));
        assert_eq!(pct(r).as_deref(), Some("20.00"));
        // Never held anything.
        assert_eq!(twr(Money::ZERO, &[], Money::ZERO), None);
        // Emptied, then value from nowhere: not measurable.
        assert_eq!(twr(Money::ZERO, &[], m("5.00")), None);
        // All sold on a flow day: the last stretch is 0 to 0.
        let r = twr(m("1000.00"), &[(Money::ZERO, m("-1100.00"))], Money::ZERO);
        assert_eq!(pct(r).as_deref(), Some("10.00"));
    }

    #[test]
    fn percent_text_rounds_half_even_and_drops_negative_zero() {
        assert_eq!(
            percent_text(Decimal::new(12345, 6)).as_deref(),
            Some("1.23")
        );
        assert_eq!(percent_text(Decimal::new(125, 5)).as_deref(), Some("0.12"));
        assert_eq!(percent_text(Decimal::new(-1, 6)).as_deref(), Some("0.00"));
    }
}
