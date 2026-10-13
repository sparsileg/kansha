//! Loan arithmetic (LOAN-020, LOAN-025, LOAN-030, LOAN-070): the rate
//! per payment, the level payment, and each payment's split into
//! interest and principal. Pure: no database, no clock.
//!
//! Balances here are what is owed, positive. Intermediate math is
//! `rust_decimal`; every amount is rounded half-even to cents (§19).

use rust_decimal::{Decimal, MathematicalOps};

use super::{Compounding, LoanRate, PaymentFrequency};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::{Money, Rate};

fn overflow() -> Error {
    Error::Overflow("loan arithmetic")
}

/// An annual rate in percent as a fraction: 6 % is 0.06.
fn fraction(rate: Rate) -> Result<Decimal> {
    rate.to_decimal()
        .checked_div(Decimal::ONE_HUNDRED)
        .ok_or_else(overflow)
}

/// The rate per payment (LOAN-020): i = (1 + r/m)^(m/p) − 1, exactly
/// r/m when m = p.
pub fn rate_per_payment(
    rate: Rate,
    compounding: Compounding,
    frequency: PaymentFrequency,
) -> Result<Decimal> {
    let m = Decimal::from(compounding.per_year());
    let p = Decimal::from(frequency.per_year());
    let per_compounding = fraction(rate)?.checked_div(m).ok_or_else(overflow)?;
    if m == p {
        return Ok(per_compounding);
    }
    let exponent = m.checked_div(p).ok_or_else(overflow)?;
    Ok((Decimal::ONE + per_compounding)
        .checked_powd(exponent)
        .ok_or_else(overflow)?
        - Decimal::ONE)
}

/// The level payment, principal and interest (LOAN-025):
/// B·i / (1 − (1 + i)^−n), rounded half-even. With no interest it is
/// B / n.
pub fn level_payment(balance: Money, i: Decimal, n: i64) -> Result<Money> {
    if n < 1 {
        return Err(Error::Invalid("a loan needs at least one payment".into()));
    }
    let b = balance.to_decimal();
    if i.is_zero() {
        return Money::from_decimal(b.checked_div(Decimal::from(n)).ok_or_else(overflow)?);
    }
    let growth = (Decimal::ONE + i).checked_powi(n).ok_or_else(overflow)?;
    let discount = Decimal::ONE - Decimal::ONE.checked_div(growth).ok_or_else(overflow)?;
    Money::from_decimal(
        b.checked_mul(i)
            .and_then(|x| x.checked_div(discount))
            .ok_or_else(overflow)?,
    )
}

/// One payment's interest on `balance` (LOAN-030). Daily compounding
/// counts the actual days since the previous payment, 365 to the year
/// in leap years too: B·((1 + r/365)^d − 1). Monthly and semi-annual
/// compounding charge B·i whatever the days.
pub fn interest(
    balance: Money,
    rate: Rate,
    compounding: Compounding,
    frequency: PaymentFrequency,
    days: i64,
) -> Result<Money> {
    let factor = match compounding {
        Compounding::Daily => {
            if days <= 0 {
                return Ok(Money::ZERO);
            }
            let daily = fraction(rate)?
                .checked_div(Decimal::from(365))
                .ok_or_else(overflow)?;
            (Decimal::ONE + daily)
                .checked_powi(days)
                .ok_or_else(overflow)?
                - Decimal::ONE
        }
        _ => rate_per_payment(rate, compounding, frequency)?,
    };
    Money::from_decimal(
        balance
            .to_decimal()
            .checked_mul(factor)
            .ok_or_else(overflow)?,
    )
}

/// The rate in effect on `date` (LOAN-050): the latest that took effect
/// on or before it; before the first, the first. `rates` is sorted by
/// date and not empty.
pub fn rate_on(rates: &[LoanRate], date: Date) -> Result<(usize, Rate)> {
    let first = rates
        .first()
        .ok_or_else(|| Error::Invalid("a loan needs an interest rate".into()))?;
    Ok(rates
        .iter()
        .enumerate()
        .rev()
        .find(|(_, r)| r.effective <= date)
        .map_or((0, first.rate), |(k, r)| (k, r.rate)))
}

/// The terms a projection runs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Projection<'a> {
    pub compounding: Compounding,
    pub frequency: PaymentFrequency,
    /// Sorted by date; the first takes effect on the opening date.
    pub rates: &'a [LoanRate],
    /// The payment, principal and interest, for each rate: `Some` when
    /// typed or already worked out, `None` to work it out from the
    /// balance and payments left when that rate's first payment comes.
    pub payments: Vec<Option<Money>>,
    /// Paid toward principal with each payment, on top of the payment.
    pub extra: Money,
    /// Owed before the first payment projected.
    pub balance: Money,
    /// The previous payment's date (the opening date before the
    /// first): daily interest counts days from it.
    pub previous: Date,
    /// The first projected payment's number, counting from 1.
    pub number: i64,
    /// The loan's length in payments: payment number `length` pays
    /// off whatever is left.
    pub length: i64,
}

/// One payment's split (LOAN-030, LOAN-070).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub number: i64,
    pub date: Date,
    /// Principal and interest paid: the payment, or less on the last.
    pub payment: Money,
    pub interest: Money,
    pub principal: Money,
    /// Extra principal paid with it.
    pub extra: Money,
    /// Owed after it.
    pub balance: Money,
    /// Interest was more than the payment: principal is 0 and the
    /// payment covers the interest only (no negative amortization).
    pub short: bool,
}

impl Step {
    /// Everything paid toward the loan: payment plus extra principal.
    pub fn total(&self) -> Money {
        self.payment + self.extra
    }
}

/// Number of payments in a loan of `months` months: months × p / 12,
/// rounded half-even.
pub fn payments_in(months: i64, frequency: PaymentFrequency) -> Result<i64> {
    crate::money::mul_div(months, frequency.per_year(), 12)
}

/// Split each payment on `dates` (in order) until the loan is paid off
/// or the dates run out (LOAN-030). Each payment's interest is on the
/// balance left by the one before, so this assumes each is made on its
/// date.
pub fn project(p: &Projection<'_>, dates: impl IntoIterator<Item = Date>) -> Result<Vec<Step>> {
    let mut payments = p.payments.clone();
    let mut balance = p.balance;
    let mut previous = p.previous;
    let mut out = Vec::new();
    for (number, date) in (p.number..).zip(dates) {
        if balance <= Money::ZERO {
            break;
        }
        let (k, rate) = rate_on(p.rates, date)?;
        let payment = match payments.get(k).copied().flatten() {
            Some(m) => m,
            None => {
                let left = (p.length - number + 1).max(1);
                let i = rate_per_payment(rate, p.compounding, p.frequency)?;
                let m = level_payment(balance, i, left)?;
                if let Some(slot) = payments.get_mut(k) {
                    *slot = Some(m);
                }
                m
            }
        };
        let days = (date.naive() - previous.naive()).num_days();
        let interest = interest(balance, rate, p.compounding, p.frequency, days)?;
        let owed = balance.checked_add(interest).ok_or_else(overflow)?;
        let due = payment.checked_add(p.extra).ok_or_else(overflow)?;
        let step = if owed <= due || number >= p.length {
            // The last payment: whatever is owed. Extra principal takes
            // only what the payment leaves; the payment takes the rest,
            // so the final payment of the length pays off the rounding.
            let regular = payment.checked_sub(interest).ok_or_else(overflow)?;
            let extra = p
                .extra
                .min(balance.checked_sub(regular).ok_or_else(overflow)?)
                .max(Money::ZERO);
            let principal = balance - extra;
            Step {
                number,
                date,
                payment: interest + principal,
                interest,
                principal,
                extra,
                balance: Money::ZERO,
                short: false,
            }
        } else if interest > payment {
            Step {
                number,
                date,
                payment: interest,
                interest,
                principal: Money::ZERO,
                extra: p.extra.min(balance),
                balance: balance - p.extra.min(balance),
                short: true,
            }
        } else {
            let principal = payment - interest;
            let paid = principal.checked_add(p.extra).ok_or_else(overflow)?;
            Step {
                number,
                date,
                payment,
                interest,
                principal,
                extra: p.extra,
                balance: balance - paid,
                short: false,
            }
        };
        balance = step.balance;
        out.push(step);
        previous = date;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    //! Expected values: published payments (300,000 at 6 % for 30
    //! years: 1,798.65; 200,000 at 4.5 % for 15 years: 1,529.99;
    //! 100,000 at 5 % for 25 years compounded semi-annually, the
    //! Canadian mortgage: 581.60), and whole tables worked out
    //! independently with Python's `decimal` at 50 digits (LOAN-080).
    use super::*;

    fn rate(s: &str) -> Rate {
        s.parse().unwrap()
    }
    fn m(s: &str) -> Money {
        s.parse().unwrap()
    }
    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    /// Monthly dates on `day` from the month after `opening`.
    fn monthly(opening: Date, day: u32, count: usize) -> Vec<Date> {
        let (mut y, mut mo) = (opening.year(), opening.month());
        (0..count)
            .map(|_| {
                mo += 1;
                if mo > 12 {
                    mo = 1;
                    y += 1;
                }
                Date::from_ymd(y, mo, day).unwrap()
            })
            .collect()
    }

    /// Every 14 days from `opening`.
    fn biweekly(opening: Date, count: usize) -> Vec<Date> {
        (1..=count as i64)
            .map(|k| crate::schedule::add_days(opening, 14 * k).unwrap())
            .collect()
    }

    fn table(
        balance: &str,
        r: &str,
        compounding: Compounding,
        frequency: PaymentFrequency,
        length: i64,
        dates: Vec<Date>,
        extra: &str,
    ) -> (Money, Vec<Step>) {
        let rates = [LoanRate {
            effective: d("2026-01-15"),
            rate: rate(r),
        }];
        let i = rate_per_payment(rate(r), compounding, frequency).unwrap();
        let payment = level_payment(m(balance), i, length).unwrap();
        let p = Projection {
            compounding,
            frequency,
            rates: &rates,
            payments: vec![Some(payment)],
            extra: m(extra),
            balance: m(balance),
            previous: d("2026-01-15"),
            number: 1,
            length,
        };
        (payment, project(&p, dates).unwrap())
    }

    fn total_interest(steps: &[Step]) -> Money {
        steps.iter().map(|s| s.interest).sum()
    }

    #[test]
    fn rate_per_payment_is_r_over_m_when_they_match() {
        let i =
            rate_per_payment(rate("6"), Compounding::Monthly, PaymentFrequency::Monthly).unwrap();
        assert_eq!(i, Decimal::new(5, 3));
    }

    #[test]
    fn rate_per_payment_takes_a_fractional_power() {
        // (1 + 0.05/2)^(1/6) − 1 = 0.0041239154651442714...
        let i = rate_per_payment(
            rate("5"),
            Compounding::SemiAnnually,
            PaymentFrequency::Monthly,
        )
        .unwrap();
        let expected = Decimal::from_str_exact("0.0041239154651442714").unwrap();
        assert!((i - expected).abs() < Decimal::new(1, 15), "{i}");
        // (1 + 0.07/365)^(365/12) − 1 = 0.0058498177919638828...
        let i = rate_per_payment(rate("7"), Compounding::Daily, PaymentFrequency::Monthly).unwrap();
        let expected = Decimal::from_str_exact("0.0058498177919638828").unwrap();
        assert!((i - expected).abs() < Decimal::new(1, 15), "{i}");
    }

    #[test]
    fn published_payments() {
        let pay = |b: &str, r: &str, c, f, n| {
            level_payment(m(b), rate_per_payment(rate(r), c, f).unwrap(), n).unwrap()
        };
        use Compounding as C;
        use PaymentFrequency as F;
        assert_eq!(
            pay("300000", "6", C::Monthly, F::Monthly, 360),
            m("1798.65")
        );
        assert_eq!(
            pay("200000", "4.5", C::Monthly, F::Monthly, 180),
            m("1529.99")
        );
        assert_eq!(
            pay("100000", "5", C::SemiAnnually, F::Monthly, 300),
            m("581.60")
        );
        assert_eq!(
            pay("300000", "6", C::Monthly, F::EveryOtherWeek, 780),
            m("829.03")
        );
        assert_eq!(pay("20000", "7", C::Daily, F::Monthly, 60), m("396.21"));
        assert_eq!(pay("1200", "0", C::Monthly, F::Monthly, 12), m("100.00"));
    }

    #[test]
    fn thirty_year_mortgage_matches_the_table_to_the_cent() {
        let open = d("2026-01-15");
        let (payment, steps) = table(
            "300000",
            "6",
            Compounding::Monthly,
            PaymentFrequency::Monthly,
            360,
            monthly(open, 15, 400),
            "0",
        );
        assert_eq!(payment, m("1798.65"));
        assert_eq!(steps.len(), 360);
        let s = steps[0];
        assert_eq!(
            (s.interest, s.principal, s.balance),
            (m("1500.00"), m("298.65"), m("299701.35"))
        );
        let s = steps[1];
        assert_eq!(
            (s.interest, s.principal, s.balance),
            (m("1498.51"), m("300.14"), m("299401.21"))
        );
        let s = steps[358];
        assert_eq!(
            (s.interest, s.principal, s.balance),
            (m("17.86"), m("1780.79"), m("1791.13"))
        );
        // Payment 360 pays off the rounding left over.
        let s = steps[359];
        assert_eq!(
            (s.payment, s.interest, s.principal, s.balance),
            (m("1800.09"), m("8.96"), m("1791.13"), m("0.00"))
        );
        assert_eq!(total_interest(&steps), m("347515.44"));
    }

    #[test]
    fn fifteen_year_last_payment_is_smaller() {
        let (_, steps) = table(
            "200000",
            "4.5",
            Compounding::Monthly,
            PaymentFrequency::Monthly,
            180,
            monthly(d("2026-01-15"), 15, 200),
            "0",
        );
        assert_eq!(steps.len(), 180);
        let s = steps[178];
        assert_eq!(
            (s.interest, s.principal, s.balance),
            (m("11.41"), m("1518.58"), m("1523.40"))
        );
        let s = steps[179];
        assert_eq!(
            (s.payment, s.interest, s.principal),
            (m("1529.11"), m("5.71"), m("1523.40"))
        );
        assert_eq!(total_interest(&steps), m("75397.32"));
    }

    #[test]
    fn semi_annual_compounding_matches_the_table() {
        let (_, steps) = table(
            "100000",
            "5",
            Compounding::SemiAnnually,
            PaymentFrequency::Monthly,
            300,
            monthly(d("2026-01-15"), 15, 320),
            "0",
        );
        assert_eq!(steps.len(), 300);
        let s = steps[0];
        assert_eq!(
            (s.interest, s.principal, s.balance),
            (m("412.39"), m("169.21"), m("99830.79"))
        );
        let s = steps[1];
        assert_eq!(
            (s.interest, s.principal, s.balance),
            (m("411.69"), m("169.91"), m("99660.88"))
        );
        assert_eq!(steps[299].balance, Money::ZERO);
    }

    #[test]
    fn biweekly_matches_the_table() {
        let (payment, steps) = table(
            "250000",
            "6",
            Compounding::Monthly,
            PaymentFrequency::EveryOtherWeek,
            780,
            biweekly(d("2026-01-15"), 800),
            "0",
        );
        // Monthly compounding, every other week: i = 1.005^(12/26) − 1.
        assert_eq!(payment, m("690.86"));
        assert_eq!(steps.len(), 780);
        assert_eq!(steps[0].interest, m("576.15"));
        assert_eq!(steps[779].balance, Money::ZERO);
        assert_eq!(total_interest(&steps), m("288870.99"));
    }

    #[test]
    fn daily_compounding_counts_actual_days() {
        let open = d("2026-01-15");
        let (payment, steps) = table(
            "20000",
            "7",
            Compounding::Daily,
            PaymentFrequency::Monthly,
            60,
            monthly(open, 15, 70),
            "0",
        );
        assert_eq!(payment, m("396.21"));
        // Jan 15 → Feb 15 is 31 days; Feb 15 → Mar 15 is 28.
        let s = steps[0];
        assert_eq!(
            (s.interest, s.principal, s.balance),
            (m("119.25"), m("276.96"), m("19723.04"))
        );
        let s = steps[1];
        assert_eq!(
            (s.interest, s.principal, s.balance),
            (m("106.18"), m("290.03"), m("19433.01"))
        );
        // 2028 is a leap year: Feb 15 → Mar 15 is 29 days, still /365.
        let s = steps[25];
        assert_eq!(
            (s.interest, s.principal, s.balance),
            (m("69.75"), m("326.46"), m("12180.35"))
        );
        let s = steps[59];
        assert_eq!(
            (s.payment, s.interest, s.principal),
            (m("394.84"), m("2.34"), m("392.50"))
        );
        assert_eq!(total_interest(&steps), m("3771.23"));
    }

    #[test]
    fn extra_principal_shortens_the_loan() {
        let (_, steps) = table(
            "300000",
            "6",
            Compounding::Monthly,
            PaymentFrequency::Monthly,
            360,
            monthly(d("2026-01-15"), 15, 400),
            "100",
        );
        assert_eq!(steps.len(), 313);
        assert_eq!(steps[0].extra, m("100.00"));
        assert_eq!(steps[0].balance, m("299601.35"));
        let last = steps[312];
        assert_eq!(
            (
                last.payment,
                last.interest,
                last.principal,
                last.extra,
                last.balance
            ),
            (m("1789.79"), m("8.90"), m("1780.89"), m("0.00"), m("0.00"))
        );
        assert_eq!(total_interest(&steps), m("294168.59"));
    }

    #[test]
    fn interest_above_the_payment_is_paid_without_principal() {
        let rates = [LoanRate {
            effective: d("2026-01-15"),
            rate: rate("12"),
        }];
        let p = Projection {
            compounding: Compounding::Monthly,
            frequency: PaymentFrequency::Monthly,
            rates: &rates,
            payments: vec![Some(m("50.00"))],
            extra: Money::ZERO,
            balance: m("10000"),
            previous: d("2026-01-15"),
            number: 1,
            length: 360,
        };
        let steps = project(&p, monthly(d("2026-01-15"), 15, 2)).unwrap();
        let s = steps[0];
        assert!(s.short);
        assert_eq!(
            (s.payment, s.interest, s.principal, s.balance),
            (m("100.00"), m("100.00"), m("0.00"), m("10000.00"))
        );
    }

    #[test]
    fn a_rate_change_recomputes_the_payment_from_the_balance_left() {
        let rates = [
            LoanRate {
                effective: d("2026-01-15"),
                rate: rate("6"),
            },
            LoanRate {
                effective: d("2027-01-01"),
                rate: rate("7"),
            },
        ];
        let p = Projection {
            compounding: Compounding::Monthly,
            frequency: PaymentFrequency::Monthly,
            rates: &rates,
            payments: vec![Some(m("1798.65")), None],
            extra: Money::ZERO,
            balance: m("300000"),
            previous: d("2026-01-15"),
            number: 1,
            length: 360,
        };
        let steps = project(&p, monthly(d("2026-01-15"), 15, 400)).unwrap();
        // Payment 12 (2027-01-15) is the first at 7 %: 348 payments left
        // on the balance after payment 11.
        let before = steps[10].balance;
        let i =
            rate_per_payment(rate("7"), Compounding::Monthly, PaymentFrequency::Monthly).unwrap();
        let recast = level_payment(before, i, 349).unwrap();
        assert_eq!(steps[11].payment, recast);
        assert_eq!(
            steps[11].interest,
            interest(
                before,
                rate("7"),
                Compounding::Monthly,
                PaymentFrequency::Monthly,
                31
            )
            .unwrap()
        );
        assert_eq!(steps.len(), 360);
        assert_eq!(steps[359].balance, Money::ZERO);
        assert!((steps[359].payment - recast).cents().abs() < 300);
    }

    #[test]
    fn rate_on_picks_the_latest_in_effect() {
        let rates = [
            LoanRate {
                effective: d("2026-01-15"),
                rate: rate("6"),
            },
            LoanRate {
                effective: d("2027-01-01"),
                rate: rate("7"),
            },
        ];
        assert_eq!(rate_on(&rates, d("2026-01-01")).unwrap(), (0, rate("6")));
        assert_eq!(rate_on(&rates, d("2026-12-31")).unwrap(), (0, rate("6")));
        assert_eq!(rate_on(&rates, d("2027-01-01")).unwrap(), (1, rate("7")));
        assert!(rate_on(&[], d("2027-01-01")).is_err());
    }

    #[test]
    fn payments_in_a_length() {
        assert_eq!(payments_in(360, PaymentFrequency::Monthly).unwrap(), 360);
        assert_eq!(
            payments_in(360, PaymentFrequency::EveryOtherWeek).unwrap(),
            780
        );
        assert_eq!(
            payments_in(360, PaymentFrequency::TwicePerYear).unwrap(),
            60
        );
        assert_eq!(payments_in(60, PaymentFrequency::Weekly).unwrap(), 260);
    }
}
