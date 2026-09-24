//! Calendar dates and the injected clock (NFR-090, TEST-020).
//!
//! Financial dates are date-only: no time, no time zone. The engine never
//! reads system time; "today" always comes from a [`Clock`].

use std::fmt;
use std::str::FromStr;

use chrono::{Datelike, NaiveDate};

use crate::error::{Error, Result};

/// A calendar date. Text form is ISO `YYYY-MM-DD`, exactly 10 characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(NaiveDate);

impl Date {
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Result<Date> {
        NaiveDate::from_ymd_opt(year, month, day)
            .map(Date)
            .ok_or_else(|| Error::Parse {
                kind: "date",
                input: format!("{year:04}-{month:02}-{day:02}"),
                reason: "no such calendar date".into(),
            })
    }

    pub fn year(self) -> i32 {
        self.0.year()
    }

    pub fn month(self) -> u32 {
        self.0.month()
    }

    pub fn day(self) -> u32 {
        self.0.day()
    }

    /// Underlying chrono date, for engine-internal calendar math.
    pub fn naive(self) -> NaiveDate {
        self.0
    }

    pub fn from_naive(date: NaiveDate) -> Date {
        Date(date)
    }
}

impl FromStr for Date {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let err = |reason: &str| Error::Parse {
            kind: "date",
            input: s.to_string(),
            reason: reason.into(),
        };
        let b = s.as_bytes();
        let shape_ok = b.len() == 10
            && b[4] == b'-'
            && b[7] == b'-'
            && b.iter()
                .enumerate()
                .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
        if !shape_ok {
            return Err(err("expected YYYY-MM-DD"));
        }
        NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map(Date)
            .map_err(|_| err("no such calendar date"))
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.format("%Y-%m-%d"))
    }
}

/// Source of "today". Inject everywhere the engine needs the current date.
pub trait Clock: Send + Sync {
    fn today(&self) -> Date;
}

/// A clock that always returns the same date. Used by tests and scenarios.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(Date);

impl FixedClock {
    pub fn new(today: Date) -> Self {
        FixedClock(today)
    }
}

impl Clock for FixedClock {
    fn today(&self) -> Date {
        self.0
    }
}

/// The real clock: the local calendar date. Constructed only by the app
/// shell; engine code receives it as `&dyn Clock`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn today(&self) -> Date {
        Date(chrono::Local::now().date_naive())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_displays_iso() {
        let d: Date = "2026-02-28".parse().unwrap();
        assert_eq!((d.year(), d.month(), d.day()), (2026, 2, 28));
        assert_eq!(d.to_string(), "2026-02-28");
        assert_eq!(
            Date::from_ymd(2024, 2, 29).unwrap().to_string(),
            "2024-02-29"
        );
    }

    #[test]
    fn rejects_bad_dates() {
        for bad in [
            "",
            "2026-2-28",
            "2026/02/28",
            " 2026-02-28",
            "2026-02-28 ",
            "2026-02-30",
            "2025-02-29",
            "2026-13-01",
            "26-02-28xx",
            "2026-02-2a",
        ] {
            assert!(bad.parse::<Date>().is_err(), "accepted {bad:?}");
        }
        assert!(Date::from_ymd(2026, 2, 30).is_err());
    }

    #[test]
    fn orders_chronologically() {
        let a: Date = "2025-12-31".parse().unwrap();
        let b: Date = "2026-01-01".parse().unwrap();
        assert!(a < b);
    }

    #[test]
    fn fixed_clock_returns_its_date() {
        let d: Date = "2026-06-30".parse().unwrap();
        let clock: &dyn Clock = &FixedClock::new(d);
        assert_eq!(clock.today(), d);
    }
}
