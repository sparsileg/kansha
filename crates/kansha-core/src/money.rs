//! Exact monetary and quantity types (NFR-030, spec §19).
//!
//! - [`Money`]: integer cents.
//! - [`Quantity`]: shares, integer scaled by 10^6.
//! - [`Price`]: per-share price, integer scaled by 10^6.
//!
//! No floating point is used anywhere. Text forms are plain decimals
//! (`"-1234.56"`); no thousands separators, no currency symbols. Display
//! formatting for humans belongs to the frontend `format` module.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};
use std::str::FromStr;

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};

use crate::error::{Error, Result};

/// Decimal places in [`Money`].
pub const MONEY_SCALE: u32 = 2;
/// Decimal places in [`Quantity`] and [`Price`].
pub const SCALE6: u32 = 6;

// ---------------------------------------------------------------------------
// Money
// ---------------------------------------------------------------------------

/// An exact amount of money in integer cents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Money(i64);

impl Money {
    pub const ZERO: Money = Money(0);

    pub const fn from_cents(cents: i64) -> Self {
        Money(cents)
    }

    pub const fn cents(self) -> i64 {
        self.0
    }

    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    pub fn to_decimal(self) -> Decimal {
        Decimal::new(self.0, MONEY_SCALE)
    }

    /// Round a decimal to cents, half-even.
    pub fn from_decimal(value: Decimal) -> Result<Money> {
        let cents = value
            .checked_mul(Decimal::ONE_HUNDRED)
            .ok_or(Error::Overflow("Money::from_decimal"))?
            .round_dp_with_strategy(0, RoundingStrategy::MidpointNearestEven);
        cents
            .to_i64()
            .map(Money)
            .ok_or(Error::Overflow("Money::from_decimal"))
    }
}

impl FromStr for Money {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        parse_fixed(s, MONEY_SCALE)
            .map(Money)
            .map_err(|reason| parse_error("amount", s, reason))
    }
}

impl fmt::Display for Money {
    /// Always two decimal places: `"12.30"`, `"-0.05"`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&format_fixed(self.0, MONEY_SCALE, false))
    }
}

// ---------------------------------------------------------------------------
// Quantity and Price
// ---------------------------------------------------------------------------

macro_rules! scaled6 {
    ($(#[$doc:meta])* $name:ident, $kind:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct $name(i64);

        impl $name {
            pub const ZERO: $name = $name(0);

            /// Construct from the raw scaled integer (value × 10^6).
            pub const fn from_raw(raw: i64) -> Self {
                $name(raw)
            }

            /// The raw scaled integer (value × 10^6).
            pub const fn raw(self) -> i64 {
                self.0
            }

            pub const fn is_negative(self) -> bool {
                self.0 < 0
            }

            pub fn to_decimal(self) -> Decimal {
                Decimal::new(self.0, SCALE6)
            }
        }

        impl FromStr for $name {
            type Err = Error;

            fn from_str(s: &str) -> Result<Self> {
                parse_fixed(s, SCALE6)
                    .map($name)
                    .map_err(|reason| parse_error($kind, s, reason))
            }
        }

        impl fmt::Display for $name {
            /// Trailing zeros trimmed: `"100"`, `"12.5"`, `"0.333333"`.
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&format_fixed(self.0, SCALE6, true))
            }
        }
    };
}

scaled6!(
    /// A quantity of shares, exact to 6 decimal places.
    Quantity,
    "quantity"
);
scaled6!(
    /// A per-share price, exact to 6 decimal places.
    Price,
    "price"
);
scaled6!(
    /// An annual interest rate in percent, exact to 6 decimal places
    /// (`"4.35"` is 4.35 %).
    Rate,
    "rate"
);

// ---------------------------------------------------------------------------
// Arithmetic (Money and Quantity; Price and Rate have none)
// ---------------------------------------------------------------------------

macro_rules! arithmetic {
    ($name:ident) => {
        impl $name {
            pub const fn is_zero(self) -> bool {
                self.0 == 0
            }

            pub fn checked_add(self, rhs: $name) -> Option<$name> {
                self.0.checked_add(rhs.0).map($name)
            }

            pub fn checked_sub(self, rhs: $name) -> Option<$name> {
                self.0.checked_sub(rhs.0).map($name)
            }

            pub fn checked_neg(self) -> Option<$name> {
                self.0.checked_neg().map($name)
            }
        }

        // Overflow of i64 cents (~$92 quadrillion) is treated as an
        // invariant violation: panic rather than wrap silently.
        impl Add for $name {
            type Output = $name;
            fn add(self, rhs: $name) -> $name {
                self.checked_add(rhs)
                    .expect(concat!(stringify!($name), " overflow"))
            }
        }

        impl Sub for $name {
            type Output = $name;
            fn sub(self, rhs: $name) -> $name {
                self.checked_sub(rhs)
                    .expect(concat!(stringify!($name), " overflow"))
            }
        }

        impl Neg for $name {
            type Output = $name;
            fn neg(self) -> $name {
                $name(
                    self.0
                        .checked_neg()
                        .expect(concat!(stringify!($name), " overflow")),
                )
            }
        }

        impl AddAssign for $name {
            fn add_assign(&mut self, rhs: $name) {
                *self = *self + rhs;
            }
        }

        impl SubAssign for $name {
            fn sub_assign(&mut self, rhs: $name) {
                *self = *self - rhs;
            }
        }

        impl Sum for $name {
            fn sum<I: Iterator<Item = $name>>(iter: I) -> $name {
                iter.fold($name::ZERO, |acc, x| acc + x)
            }
        }

        impl<'a> Sum<&'a $name> for $name {
            fn sum<I: Iterator<Item = &'a $name>>(iter: I) -> $name {
                iter.copied().sum()
            }
        }
    };
}

arithmetic!(Money);
arithmetic!(Quantity);

/// Quantity × price, rounded half-even to cents.
pub fn extended_value(quantity: Quantity, price: Price) -> Result<Money> {
    // raw × raw is scaled by 10^12 dollars; cents need 10^2 → divide by 10^10.
    let product = i128::from(quantity.0) * i128::from(price.0);
    let cents = div_round_half_even(product, 10_i128.pow(10));
    i64::try_from(cents)
        .map(Money)
        .map_err(|_| Error::Overflow("extended_value"))
}

/// `value × num ÷ den`, rounded half-even, without intermediate overflow.
/// `den` must be positive.
pub fn mul_div(value: i64, num: i64, den: i64) -> Result<i64> {
    if den <= 0 {
        return Err(Error::Invalid("division by a non-positive number".into()));
    }
    let q = div_round_half_even(i128::from(value) * i128::from(num), i128::from(den));
    i64::try_from(q).map_err(|_| Error::Overflow("mul_div"))
}

/// Split `total` into parts proportional to `weights` that add up to
/// `total` exactly (spec §19, LOT-030). Each part is first rounded down;
/// the units left over go one each to the parts with the largest
/// remainders, ties to the earlier part. `total` and every weight must be
/// zero or more, and the weights must not all be zero.
pub fn allocate(total: i64, weights: &[i64]) -> Result<Vec<i64>> {
    if total < 0 || weights.iter().any(|w| *w < 0) {
        return Err(Error::Invalid(
            "allocation needs non-negative numbers".into(),
        ));
    }
    let sum: i128 = weights.iter().map(|w| i128::from(*w)).sum();
    if sum == 0 {
        return Err(Error::Invalid("allocation needs a non-zero weight".into()));
    }
    let mut parts = Vec::with_capacity(weights.len());
    let mut remainders = Vec::with_capacity(weights.len());
    for (i, w) in weights.iter().enumerate() {
        let n = i128::from(total) * i128::from(*w);
        parts.push(n / sum);
        remainders.push((n % sum, i));
    }
    let given: i128 = parts.iter().sum();
    let left =
        usize::try_from(i128::from(total) - given).map_err(|_| Error::Overflow("allocate"))?;
    // Largest remainder first; equal remainders keep their order.
    remainders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (_, i) in remainders.into_iter().take(left) {
        parts[i] += 1;
    }
    parts
        .into_iter()
        .map(|p| i64::try_from(p).map_err(|_| Error::Overflow("allocate")))
        .collect()
}

impl Price {
    /// Cost per share: `basis ÷ quantity`, rounded half-even to 6 decimal
    /// places. `None` for no shares.
    pub fn per_share(basis: Money, quantity: Quantity) -> Result<Option<Price>> {
        if quantity.raw() <= 0 {
            return Ok(None);
        }
        // cents × 10^10 / (shares × 10^6) = dollars × 10^6 per share.
        mul_div(basis.cents(), 10_i64.pow(10), quantity.raw()).map(|p| Some(Price(p)))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_error(kind: &'static str, input: &str, reason: String) -> Error {
    Error::Parse {
        kind,
        input: input.to_string(),
        reason,
    }
}

/// Parse a plain decimal string into an integer scaled by 10^scale.
///
/// Accepts `[-]digits[.digits]` with at most `scale` fraction digits.
/// Rejects whitespace, `+`, separators, and bare `.5` / `5.` forms.
fn parse_fixed(input: &str, scale: u32) -> std::result::Result<i64, String> {
    let (negative, body) = match input.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, input),
    };
    let (int_part, frac_part) = match body.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (body, None),
    };
    if int_part.is_empty() || !int_part.bytes().all(|b| b.is_ascii_digit()) {
        return Err("expected digits before the decimal point".into());
    }
    let frac = match frac_part {
        None => "",
        Some(f) if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) => {
            return Err("expected digits after the decimal point".into());
        }
        Some(f) if f.len() > scale as usize => {
            return Err(format!("more than {scale} decimal places"));
        }
        Some(f) => f,
    };

    let limit = i128::from(i64::MAX) + 1;
    let mut value: i128 = 0;
    for b in int_part.bytes().chain(frac.bytes()) {
        value = value * 10 + i128::from(b - b'0');
        if value > limit {
            return Err("out of range".into());
        }
    }
    for _ in frac.len()..scale as usize {
        value *= 10;
    }
    if negative {
        value = -value;
    }
    i64::try_from(value).map_err(|_| "out of range".into())
}

/// Format an integer scaled by 10^scale as a plain decimal string.
fn format_fixed(value: i64, scale: u32, trim: bool) -> String {
    let divisor = 10_u64.pow(scale);
    let abs = value.unsigned_abs();
    let mut out = String::new();
    if value < 0 {
        out.push('-');
    }
    out.push_str(&(abs / divisor).to_string());
    if scale > 0 {
        let mut frac = format!("{:0width$}", abs % divisor, width = scale as usize);
        if trim {
            while frac.ends_with('0') {
                frac.pop();
            }
        }
        if !frac.is_empty() {
            out.push('.');
            out.push_str(&frac);
        }
    }
    out
}

/// Integer division rounding half to even. `d` must be positive.
fn div_round_half_even(n: i128, d: i128) -> i128 {
    assert!(d > 0, "divisor must be positive");
    let q = n.div_euclid(d);
    let twice_r = 2 * n.rem_euclid(d);
    if twice_r > d || (twice_r == d && q % 2 != 0) {
        q + 1
    } else {
        q
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn m(s: &str) -> Money {
        s.parse().unwrap()
    }

    fn q(s: &str) -> Quantity {
        s.parse().unwrap()
    }

    fn p(s: &str) -> Price {
        s.parse().unwrap()
    }

    #[test]
    fn money_parses_valid_forms() {
        assert_eq!(m("0").cents(), 0);
        assert_eq!(m("12").cents(), 1200);
        assert_eq!(m("12.3").cents(), 1230);
        assert_eq!(m("12.34").cents(), 1234);
        assert_eq!(m("-0.05").cents(), -5);
        assert_eq!(m("-0").cents(), 0);
    }

    #[test]
    fn money_rejects_invalid_forms() {
        for bad in [
            "", "-", ".5", "5.", "1.234", "1,000.00", " 1.00", "1.00 ", "+1", "$1", "1e3", "--1",
            "1.-5", "abc",
        ] {
            assert!(bad.parse::<Money>().is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn money_parse_range_limits() {
        assert_eq!(m("92233720368547758.07").cents(), i64::MAX);
        assert_eq!(m("-92233720368547758.08").cents(), i64::MIN);
        assert!("92233720368547758.08".parse::<Money>().is_err());
        assert!("99999999999999999999999999".parse::<Money>().is_err());
    }

    #[test]
    fn money_display() {
        assert_eq!(m("12.3").to_string(), "12.30");
        assert_eq!(m("-0.05").to_string(), "-0.05");
        assert_eq!(m("0").to_string(), "0.00");
        assert_eq!(
            Money::from_cents(i64::MIN).to_string(),
            "-92233720368547758.08"
        );
    }

    #[test]
    fn money_arithmetic_is_exact() {
        // The classic float trap: 0.10 + 0.20 == 0.30 exactly.
        assert_eq!(m("0.10") + m("0.20"), m("0.30"));
        assert_eq!(m("100.00") - m("100.01"), m("-0.01"));
        assert_eq!(-m("5.00"), m("-5.00"));
        let total: Money = [m("1.10"), m("2.20"), m("3.30")].iter().sum();
        assert_eq!(total, m("6.60"));
        assert_eq!(Money::from_cents(i64::MAX).checked_add(m("0.01")), None);
    }

    #[test]
    #[should_panic(expected = "Money overflow")]
    fn money_overflow_panics() {
        let _ = Money::from_cents(i64::MAX) + m("0.01");
    }

    #[test]
    fn money_from_decimal_rounds_half_even() {
        let d = |s: &str| s.parse::<Decimal>().unwrap();
        assert_eq!(Money::from_decimal(d("0.125")).unwrap(), m("0.12"));
        assert_eq!(Money::from_decimal(d("0.135")).unwrap(), m("0.14"));
        assert_eq!(Money::from_decimal(d("-0.125")).unwrap(), m("-0.12"));
        assert_eq!(Money::from_decimal(d("0.1251")).unwrap(), m("0.13"));
        assert_eq!(m("12.34").to_decimal(), d("12.34"));
    }

    #[test]
    fn quantity_and_price_parse_and_display() {
        assert_eq!(q("100").raw(), 100_000_000);
        assert_eq!(q("0.333333").raw(), 333_333);
        assert_eq!(q("100").to_string(), "100");
        assert_eq!(q("12.500000").to_string(), "12.5");
        assert_eq!(p("200.00").to_string(), "200");
        assert!("1.1234567".parse::<Quantity>().is_err());
        let err = "x".parse::<Price>().unwrap_err();
        assert!(err.to_string().starts_with("invalid price \"x\""), "{err}");
    }

    #[test]
    fn extended_value_rounds_half_even() {
        assert_eq!(
            extended_value(q("100"), p("200.00")).unwrap(),
            m("20000.00")
        );
        assert_eq!(extended_value(q("0.333333"), p("3")).unwrap(), m("1.00"));
        // 1.5 cents → 2 (even); 2.5 cents → 2 (even)
        assert_eq!(extended_value(q("1.5"), p("0.01")).unwrap(), m("0.02"));
        assert_eq!(extended_value(q("2.5"), p("0.01")).unwrap(), m("0.02"));
        assert_eq!(extended_value(q("-2.5"), p("0.01")).unwrap(), m("-0.02"));
        assert!(extended_value(Quantity::from_raw(i64::MAX), Price::from_raw(i64::MAX)).is_err());
    }

    #[test]
    fn mul_div_rounds_half_even() {
        assert_eq!(mul_div(10000, 1, 3).unwrap(), 3333);
        assert_eq!(mul_div(5, 1, 2).unwrap(), 2);
        assert_eq!(mul_div(7, 1, 2).unwrap(), 4);
        assert_eq!(mul_div(i64::MAX, 2, 2).unwrap(), i64::MAX);
        assert!(mul_div(i64::MAX, 3, 1).is_err());
        assert!(mul_div(1, 1, 0).is_err());
    }

    #[test]
    fn allocate_sums_exactly_and_is_deterministic() {
        assert_eq!(allocate(100, &[1, 1, 1]).unwrap(), vec![34, 33, 33]);
        assert_eq!(allocate(1001, &[3, 3, 4]).unwrap(), vec![300, 300, 401]);
        assert_eq!(allocate(2, &[1, 1, 1]).unwrap(), vec![1, 1, 0]);
        assert_eq!(allocate(0, &[5, 0]).unwrap(), vec![0, 0]);
        assert_eq!(allocate(7, &[0, 2]).unwrap(), vec![0, 7]);
        assert!(allocate(1, &[0, 0]).is_err());
        assert!(allocate(-1, &[1]).is_err());
        assert!(allocate(1, &[-1, 2]).is_err());
    }

    #[test]
    fn per_share_cost() {
        assert_eq!(
            Price::per_share(m("1000.00"), q("3")).unwrap(),
            Some(p("333.333333"))
        );
        assert_eq!(
            Price::per_share(m("10.00"), q("4")).unwrap(),
            Some(p("2.5"))
        );
        assert_eq!(Price::per_share(m("10.00"), Quantity::ZERO).unwrap(), None);
    }

    #[test]
    fn div_round_half_even_cases() {
        assert_eq!(div_round_half_even(5, 2), 2);
        assert_eq!(div_round_half_even(7, 2), 4);
        assert_eq!(div_round_half_even(-5, 2), -2);
        assert_eq!(div_round_half_even(-7, 2), -4);
        assert_eq!(div_round_half_even(-6, 4), -2);
        assert_eq!(div_round_half_even(1, 3), 0);
        assert_eq!(div_round_half_even(2, 3), 1);
        assert_eq!(div_round_half_even(-2, 3), -1);
    }
}
