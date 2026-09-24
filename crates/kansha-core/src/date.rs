//! Calendar dates and the injected clock (NFR-090, TEST-020).
//!
//! Financial dates are date-only: no time, no time zone. The engine never
//! reads system time; "today" always comes from a [`Clock`].

use std::fmt;
use std::str::FromStr;

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike};

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

/// A UTC instant to the second, for record-keeping (audit log, created_at).
/// Never used for financial dates. Text form is `YYYY-MM-DDTHH:MM:SSZ`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(NaiveDateTime);

const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%SZ";

impl Timestamp {
    /// Midnight UTC at the start of `date`.
    pub fn start_of(date: Date) -> Timestamp {
        Timestamp(date.0.and_time(NaiveTime::MIN))
    }

    pub fn from_ymd_hms(
        year: i32,
        month: u32,
        day: u32,
        h: u32,
        m: u32,
        s: u32,
    ) -> Result<Timestamp> {
        NaiveDate::from_ymd_opt(year, month, day)
            .and_then(|d| d.and_hms_opt(h, m, s))
            .map(Timestamp)
            .ok_or_else(|| Error::Parse {
                kind: "timestamp",
                input: format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z"),
                reason: "no such instant".into(),
            })
    }
}

impl FromStr for Timestamp {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let parsed = NaiveDateTime::parse_from_str(s, TIMESTAMP_FORMAT)
            .ok()
            .map(Timestamp)
            // Round-trip check rejects non-canonical forms (e.g. `2026-9-1T…`).
            .filter(|t| t.to_string() == s);
        parsed.ok_or_else(|| Error::Parse {
            kind: "timestamp",
            input: s.to_string(),
            reason: "expected YYYY-MM-DDTHH:MM:SSZ".into(),
        })
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.format(TIMESTAMP_FORMAT))
    }
}

/// Source of "today" and "now". Inject everywhere the engine needs the
/// current date or time.
pub trait Clock: Send + Sync {
    /// The current calendar date (local), for financial logic.
    fn today(&self) -> Date;

    /// The current UTC instant, for record-keeping only.
    fn now(&self) -> Timestamp;
}

/// A clock that always returns the same date and instant. Used by tests
/// and scenarios.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    today: Date,
    now: Timestamp,
}

impl FixedClock {
    /// `now` is midnight UTC at the start of `today`.
    pub fn new(today: Date) -> Self {
        FixedClock {
            today,
            now: Timestamp::start_of(today),
        }
    }

    pub fn with_now(today: Date, now: Timestamp) -> Self {
        FixedClock { today, now }
    }
}

impl Clock for FixedClock {
    fn today(&self) -> Date {
        self.today
    }

    fn now(&self) -> Timestamp {
        self.now
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

    fn now(&self) -> Timestamp {
        // Truncated to whole seconds to match the stored text form.
        let now = chrono::Utc::now().naive_utc();
        Timestamp(now.with_nanosecond(0).unwrap_or(now))
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
        assert_eq!(clock.now().to_string(), "2026-06-30T00:00:00Z");
    }

    #[test]
    fn timestamp_round_trips_and_rejects_bad_forms() {
        let t: Timestamp = "2026-09-24T01:55:25Z".parse().unwrap();
        assert_eq!(t.to_string(), "2026-09-24T01:55:25Z");
        assert_eq!(Timestamp::from_ymd_hms(2026, 9, 24, 1, 55, 25).unwrap(), t);
        for bad in [
            "",
            "2026-09-24T01:55:25",
            "2026-09-24 01:55:25Z",
            "2026-9-24T01:55:25Z",
            "2026-02-30T00:00:00Z",
            "2026-09-24T24:00:00Z",
            "2026-09-24T01:55:25.5Z",
        ] {
            assert!(bad.parse::<Timestamp>().is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn system_clock_now_has_whole_seconds() {
        let now = SystemClock.now().to_string();
        assert_eq!(now.parse::<Timestamp>().unwrap().to_string(), now);
    }
}
