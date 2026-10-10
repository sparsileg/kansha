//! Amount sums typed in an amount field (REG-035): `"2.99+3.39"` →
//! `6.38`. The UI does no money arithmetic, so it asks here.
//!
//! Grammar, spaces ignored:
//!
//! ```text
//! sum    = [sign] term { ("+" | "-") term }
//! term   = number { ("*" | "/") number }
//! number = digits with optional thousands commas, optional "." fraction;
//!          or "." fraction alone; an optional leading "$"
//! ```
//!
//! `*` and `/` bind tighter than `+` and `-`; each level works left to
//! right. No parentheses. Numbers may have any number of decimals; the
//! exact result (`rust_decimal`) is rounded to cents half-even once, at
//! the end.

use std::str::FromStr;

use rust_decimal::Decimal;

use crate::error::{Error, Result};
use crate::money::Money;

/// Evaluate a typed sum to an amount, rounded to cents half-even. A plain
/// number is a sum of one term. Signed: the caller decides whether a
/// negative result is allowed.
pub fn eval_amount(text: &str) -> Result<Money> {
    let tokens = tokenize(text)?;
    let mut p = Parser {
        tokens: &tokens,
        pos: 0,
    };
    let value = p.sum()?;
    if p.pos < tokens.len() {
        return Err(invalid(text, "unexpected operator"));
    }
    Money::from_decimal(value).map_err(|_| invalid(text, "the result is too large"))
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Num(Decimal),
    Op(char),
}

fn invalid(text: &str, reason: &str) -> Error {
    Error::Invalid(format!("cannot work out {:?}: {reason}", text.trim()))
}

fn tokenize(text: &str) -> Result<Vec<Token>> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if matches!(c, '+' | '-' | '*' | '/') {
            out.push(Token::Op(c));
            i += 1;
        } else if c == '$' || c.is_ascii_digit() || c == '.' {
            let start = if c == '$' { i + 1 } else { i };
            let mut end = start;
            while end < chars.len()
                && (chars[end].is_ascii_digit() || matches!(chars[end], ',' | '.'))
            {
                end += 1;
            }
            let raw: String = chars[start..end].iter().collect();
            out.push(Token::Num(number(text, &raw)?));
            i = end;
        } else {
            return Err(invalid(text, &format!("{c:?} is not a number or + - * /")));
        }
    }
    if out.is_empty() {
        return Err(invalid(text, "enter an amount"));
    }
    Ok(out)
}

/// One number: "1,234.5", "12.0825", ".5". Commas, when used, group every
/// three digits.
fn number(text: &str, raw: &str) -> Result<Decimal> {
    let (int, frac) = match raw.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (raw, None),
    };
    let int_ok = if int.contains(',') {
        let groups: Vec<&str> = int.split(',').collect();
        (1..=3).contains(&groups[0].len())
            && groups[1..].iter().all(|g| g.len() == 3)
            && groups.iter().all(|g| g.bytes().all(|b| b.is_ascii_digit()))
    } else {
        int.bytes().all(|b| b.is_ascii_digit())
    };
    let frac_ok = frac.is_none_or(|f| f.bytes().all(|b| b.is_ascii_digit()));
    let has_digits = !int.is_empty() || frac.is_some_and(|f| !f.is_empty());
    if !(int_ok && frac_ok && has_digits) {
        return Err(invalid(text, &format!("{raw:?} is not a number")));
    }
    let plain = format!("{}.{}", int.replace(',', ""), frac.unwrap_or(""));
    let plain = plain.trim_end_matches('.');
    let plain = if plain.starts_with('.') {
        format!("0{plain}")
    } else {
        plain.to_string()
    };
    Decimal::from_str(&plain).map_err(|_| invalid(text, &format!("{raw:?} is too large")))
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl Parser<'_> {
    fn peek_op(&self) -> Option<char> {
        match self.tokens.get(self.pos) {
            Some(Token::Op(c)) => Some(*c),
            _ => None,
        }
    }

    fn sum(&mut self) -> Result<Decimal> {
        let negative = match self.peek_op() {
            Some(c @ ('+' | '-')) => {
                self.pos += 1;
                c == '-'
            }
            _ => false,
        };
        let mut acc = self.term()?;
        if negative {
            acc = -acc;
        }
        while let Some(c @ ('+' | '-')) = self.peek_op() {
            self.pos += 1;
            let rhs = self.term()?;
            acc = if c == '+' {
                acc.checked_add(rhs)
            } else {
                acc.checked_sub(rhs)
            }
            .ok_or_else(|| self.too_large())?;
        }
        Ok(acc)
    }

    fn term(&mut self) -> Result<Decimal> {
        let mut acc = self.number()?;
        while let Some(c @ ('*' | '/')) = self.peek_op() {
            self.pos += 1;
            let rhs = self.number()?;
            acc = if c == '*' {
                acc.checked_mul(rhs).ok_or_else(|| self.too_large())?
            } else if rhs.is_zero() {
                return Err(Error::Invalid("cannot divide by zero".into()));
            } else {
                acc.checked_div(rhs).ok_or_else(|| self.too_large())?
            };
        }
        Ok(acc)
    }

    fn number(&mut self) -> Result<Decimal> {
        match self.tokens.get(self.pos) {
            Some(Token::Num(n)) => {
                self.pos += 1;
                Ok(*n)
            }
            Some(Token::Op(c)) => Err(Error::Invalid(format!("a number is missing before {c:?}"))),
            None => Err(Error::Invalid("a number is missing at the end".into())),
        }
    }

    fn too_large(&self) -> Error {
        Error::Invalid("the result is too large".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> String {
        eval_amount(s).unwrap().to_string()
    }

    fn err(s: &str) -> String {
        eval_amount(s).unwrap_err().to_string()
    }

    #[test]
    fn plain_numbers() {
        assert_eq!(ok("5"), "5.00");
        assert_eq!(ok("1,234.5"), "1234.50");
        assert_eq!(ok(".5"), "0.50");
        assert_eq!(ok("5."), "5.00");
        assert_eq!(ok("$12.30"), "12.30");
        assert_eq!(ok("+7.1"), "7.10");
        assert_eq!(ok("-5"), "-5.00");
    }

    #[test]
    fn add_and_subtract() {
        assert_eq!(ok("2.99+3.39"), "6.38");
        assert_eq!(ok("10-2.5"), "7.50");
        assert_eq!(ok(" 1 + 2 - 3 "), "0.00");
        assert_eq!(ok("3-10"), "-7.00");
        assert_eq!(ok("-5+2"), "-3.00");
        assert_eq!(ok("1,000+$2,000.01"), "3000.01");
    }

    #[test]
    fn multiply_and_divide_bind_tighter() {
        assert_eq!(ok("2+3*4"), "14.00");
        assert_eq!(ok("10-6/2"), "7.00");
        assert_eq!(ok("8/2*3"), "12.00"); // left to right
        assert_eq!(ok("2*3+4*5"), "26.00");
        assert_eq!(ok("-2*3"), "-6.00");
    }

    #[test]
    fn rounds_once_at_the_end_half_even() {
        assert_eq!(ok("10/3"), "3.33");
        assert_eq!(ok("10/3*3"), "10.00"); // no early rounding
        assert_eq!(ok("12.99*1.0825"), "14.06"); // 14.061675
        assert_eq!(ok("0.125"), "0.12"); // half to even, down
        assert_eq!(ok("0.135"), "0.14"); // half to even, up
        assert_eq!(ok("-0.125"), "-0.12");
        assert_eq!(ok("0.25/2"), "0.12");
    }

    #[test]
    fn divide_by_zero() {
        assert_eq!(err("5/0"), "cannot divide by zero");
        assert_eq!(err("5/0.00"), "cannot divide by zero");
    }

    #[test]
    fn malformed() {
        assert_eq!(err(""), "cannot work out \"\": enter an amount");
        assert_eq!(err("   "), "cannot work out \"\": enter an amount");
        assert_eq!(err("1+"), "a number is missing at the end");
        assert_eq!(err("2++3"), "a number is missing before '+'");
        assert_eq!(err("*3"), "a number is missing before '*'");
        assert_eq!(err("5*-2"), "a number is missing before '-'");
        assert_eq!(err("2 3"), "cannot work out \"2 3\": unexpected operator");
        assert_eq!(
            err("2x3"),
            "cannot work out \"2x3\": 'x' is not a number or + - * /"
        );
        assert_eq!(
            err("1.2.3"),
            "cannot work out \"1.2.3\": \"1.2.3\" is not a number"
        );
        assert_eq!(
            err("1,23"),
            "cannot work out \"1,23\": \"1,23\" is not a number"
        );
        assert_eq!(err("."), "cannot work out \".\": \".\" is not a number");
        assert_eq!(err("$"), "cannot work out \"$\": \"\" is not a number");
        assert_eq!(
            err("(1+2)"),
            "cannot work out \"(1+2)\": '(' is not a number or + - * /"
        );
    }

    #[test]
    fn too_large() {
        assert_eq!(
            err("99999999999999999999999999999999"),
            "cannot work out \"99999999999999999999999999999999\": \"99999999999999999999999999999999\" is too large"
        );
        assert_eq!(
            err("9999999999999999999999999999*10"),
            "the result is too large"
        );
        // Fits a Decimal, not i64 cents.
        assert_eq!(
            err("100000000000000000*1"),
            "cannot work out \"100000000000000000*1\": the result is too large"
        );
    }

    #[test]
    fn huge_decimals_are_fine() {
        assert_eq!(ok("1/7*7"), "1.00");
        assert_eq!(ok("0.0000000001+1"), "1.00");
    }
}
