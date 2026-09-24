//! The recurrence engine (REC-020, REC-040, REC-050).
//!
//! A [`Recurrence`] describes a series of *nominal* dates: the dates the
//! pattern produces before any weekend shift. [`Recurrence::dates_from`]
//! generates them in order; [`Recurrence::due_date`] applies the weekend
//! rule. The series depends only on the recurrence itself, never on what
//! has been entered, so it is pure and testable without a database.
//!
//! The series starts at `start_date`: the first date is the first date the
//! pattern produces on or after it. Days that do not exist in a month
//! clamp to the month's last day (REC-040); a yearly Feb 29 becomes Feb 28
//! in other years.

use chrono::{Datelike, NaiveDate, TimeDelta, Weekday};
use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::error::{Error, Result};
use crate::text_enum::text_enum;

text_enum! {
    /// How often a schedule repeats (REC-020). Quarterly and twice a year
    /// are `Monthly` with interval 3 and 6.
    pub enum Frequency {
        Once = "once",
        Daily = "daily",
        Weekly = "weekly",
        TwiceMonthly = "twice_monthly",
        Monthly = "monthly",
        MonthlyLastDay = "monthly_last_day",
        MonthlyNthWeekday = "monthly_nth_weekday",
        Yearly = "yearly",
    }
}

text_enum! {
    /// What to do when a due date falls on a weekend (REC-050).
    pub enum WeekendRule {
        None = "none",
        Previous = "previous",
        Next = "next",
    }
}

/// Largest interval accepted; keeps date arithmetic far from overflow.
const MAX_INTERVAL: i64 = 1200;

/// A repeating pattern. Field use by frequency:
///
/// | frequency | fields |
/// |---|---|
/// | once | `start_date` |
/// | daily, weekly, monthly_last_day, yearly | `interval` |
/// | twice_monthly | `day1`, `day2` (`day1 < day2`) |
/// | monthly | `day1`, `interval` |
/// | monthly_nth_weekday | `weekday` (1 = Monday … 7 = Sunday), `week_of_month` (1–4, or −1 = last), `interval` |
///
/// Weekly repeats on `start_date`'s weekday; yearly on its month and day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Recurrence {
    pub frequency: Frequency,
    pub interval: i64,
    pub day1: Option<i64>,
    pub day2: Option<i64>,
    pub weekday: Option<i64>,
    pub week_of_month: Option<i64>,
    pub start_date: Date,
    pub weekend_rule: WeekendRule,
}

impl Recurrence {
    /// A recurrence with interval 1 and no weekend shift.
    pub fn new(frequency: Frequency, start_date: Date) -> Recurrence {
        Recurrence {
            frequency,
            interval: 1,
            day1: None,
            day2: None,
            weekday: None,
            week_of_month: None,
            start_date,
            weekend_rule: WeekendRule::None,
        }
    }

    /// Check the field combination against the frequency (mirrors the
    /// schema's CHECKs, with messages fit for display).
    pub fn validate(&self) -> Result<()> {
        let bad = |m: &str| Err(Error::Invalid(m.to_string()));
        if !(1..=MAX_INTERVAL).contains(&self.interval) {
            return bad("the repeat interval must be between 1 and 1200");
        }
        let day_ok = |d: Option<i64>| d.is_some_and(|d| (1..=31).contains(&d));
        let f = self.frequency;
        if matches!(f, Frequency::Once | Frequency::TwiceMonthly) && self.interval != 1 {
            return bad("this frequency does not take an interval");
        }
        match f {
            Frequency::Monthly if !day_ok(self.day1) => {
                return bad("choose a day of the month (1-31)");
            }
            Frequency::TwiceMonthly => {
                if !day_ok(self.day1) || !day_ok(self.day2) {
                    return bad("choose two days of the month (1-31)");
                }
                if self.day1 >= self.day2 {
                    return bad("the first day of the month must come before the second");
                }
            }
            _ => {}
        }
        if !matches!(f, Frequency::Monthly | Frequency::TwiceMonthly) && self.day1.is_some() {
            return bad("day of month applies only to monthly frequencies");
        }
        if f != Frequency::TwiceMonthly && self.day2.is_some() {
            return bad("a second day applies only to twice a month");
        }
        if f == Frequency::MonthlyNthWeekday {
            if !self.weekday.is_some_and(|d| (1..=7).contains(&d)) {
                return bad("choose a weekday");
            }
            if !self
                .week_of_month
                .is_some_and(|w| matches!(w, 1 | 2 | 3 | 4 | -1))
            {
                return bad("choose which week of the month: first to fourth, or last");
            }
        } else if self.weekday.is_some() || self.week_of_month.is_some() {
            return bad("weekday and week of month apply only to the Nth-weekday frequency");
        }
        Ok(())
    }

    /// Nominal dates on or after `from`, in order. Ends only at the
    /// calendar's limit; callers apply end conditions.
    pub fn dates_from(&self, from: Date) -> Dates {
        let period = self.first_period(from);
        Dates {
            rec: self.clone(),
            from,
            period,
            queue: Vec::new(),
            done: false,
        }
    }

    /// The first nominal date on or after `from`.
    pub fn first_on_or_after(&self, from: Date) -> Option<Date> {
        self.dates_from(from).next()
    }

    /// Is `date` in the series?
    pub fn contains(&self, date: Date) -> bool {
        self.first_on_or_after(date) == Some(date)
    }

    /// The date a nominal date is actually due, after the weekend rule
    /// (REC-050). Saturday and Sunday move to Friday or Monday.
    pub fn due_date(&self, nominal: Date) -> Date {
        let n = nominal.naive();
        let shift = match (self.weekend_rule, n.weekday()) {
            (WeekendRule::Previous, Weekday::Sat) => -1,
            (WeekendRule::Previous, Weekday::Sun) => -2,
            (WeekendRule::Next, Weekday::Sat) => 2,
            (WeekendRule::Next, Weekday::Sun) => 1,
            _ => 0,
        };
        add_days(nominal, shift).unwrap_or(nominal)
    }

    /// "How often" text for lists (REC-300).
    pub fn describe(&self) -> String {
        let every = |unit: &str, plural: &str| {
            if self.interval == 1 {
                format!("Every {unit}")
            } else {
                format!("Every {} {plural}", self.interval)
            }
        };
        match self.frequency {
            Frequency::Once => "Only once".into(),
            Frequency::Daily => every("day", "days"),
            Frequency::Weekly => {
                let day = weekday_name(i64::from(
                    self.start_date.naive().weekday().number_from_monday(),
                ));
                let base = every("week", "weeks");
                format!("{base} on {day}")
            }
            Frequency::TwiceMonthly => format!(
                "Twice a month, {} and {}",
                ordinal(self.day1.unwrap_or(0)),
                ordinal(self.day2.unwrap_or(0))
            ),
            Frequency::Monthly => match self.interval {
                1 => format!("Monthly on the {}", ordinal(self.day1.unwrap_or(0))),
                3 => format!("Quarterly on the {}", ordinal(self.day1.unwrap_or(0))),
                6 => format!("Twice a year on the {}", ordinal(self.day1.unwrap_or(0))),
                n => format!(
                    "Every {n} months on the {}",
                    ordinal(self.day1.unwrap_or(0))
                ),
            },
            Frequency::MonthlyLastDay => {
                if self.interval == 1 {
                    "Monthly on the last day".into()
                } else {
                    format!("Every {} months on the last day", self.interval)
                }
            }
            Frequency::MonthlyNthWeekday => {
                let week = match self.week_of_month {
                    Some(-1) => "last".to_string(),
                    Some(n) => ordinal_word(n).to_string(),
                    None => String::new(),
                };
                let day = weekday_name(self.weekday.unwrap_or(1));
                if self.interval == 1 {
                    format!("The {week} {day} of every month")
                } else {
                    format!("The {week} {day} every {} months", self.interval)
                }
            }
            Frequency::Yearly => {
                if self.interval == 1 {
                    "Yearly".into()
                } else {
                    format!("Every {} years", self.interval)
                }
            }
        }
    }

    /// Index of the first period that can contain a date on or after
    /// `from`, rounded down so no candidate is skipped.
    fn first_period(&self, from: Date) -> i64 {
        let s = self.start_date;
        if from <= s || self.frequency == Frequency::Once {
            return 0;
        }
        let days = (from.naive() - s.naive()).num_days();
        let months = i64::from(from.year()) * 12 + i64::from(from.month())
            - (i64::from(s.year()) * 12 + i64::from(s.month()));
        let years = i64::from(from.year()) - i64::from(s.year());
        let n = self.interval.max(1);
        match self.frequency {
            Frequency::Once => 0,
            Frequency::Daily => days / n,
            Frequency::Weekly => days / (7 * n),
            Frequency::TwiceMonthly
            | Frequency::Monthly
            | Frequency::MonthlyLastDay
            | Frequency::MonthlyNthWeekday => (months / n - 1).max(0),
            Frequency::Yearly => (years / n - 1).max(0),
        }
    }

    /// Candidate dates of period `p`, ascending; `None` past the
    /// calendar's range.
    fn period_dates(&self, p: i64) -> Option<Vec<Date>> {
        let s = self.start_date;
        let step = p.checked_mul(self.interval)?;
        match self.frequency {
            Frequency::Once => Some(vec![s]),
            Frequency::Daily => Some(vec![add_days(s, step)?]),
            Frequency::Weekly => Some(vec![add_days(s, step.checked_mul(7)?)?]),
            Frequency::Yearly => {
                let year = i32::try_from(i64::from(s.year()).checked_add(step)?).ok()?;
                Some(vec![clamped(year, s.month(), s.day())?])
            }
            Frequency::TwiceMonthly
            | Frequency::Monthly
            | Frequency::MonthlyLastDay
            | Frequency::MonthlyNthWeekday => {
                let index =
                    (i64::from(s.year()) * 12 + i64::from(s.month() - 1)).checked_add(step)?;
                let year = i32::try_from(index.div_euclid(12)).ok()?;
                let month = u32::try_from(index.rem_euclid(12)).ok()? + 1;
                self.month_dates(year, month)
            }
        }
    }

    fn month_dates(&self, year: i32, month: u32) -> Option<Vec<Date>> {
        let day = |d: Option<i64>| u32::try_from(d?).ok();
        match self.frequency {
            Frequency::Monthly => Some(vec![clamped(year, month, day(self.day1)?)?]),
            Frequency::TwiceMonthly => {
                let a = clamped(year, month, day(self.day1)?)?;
                let b = clamped(year, month, day(self.day2)?)?;
                // Both days can clamp to the month's last day (30 and 31
                // in February): one occurrence, not two.
                Some(if a == b { vec![a] } else { vec![a, b] })
            }
            Frequency::MonthlyLastDay => Some(vec![clamped(year, month, 31)?]),
            Frequency::MonthlyNthWeekday => {
                let weekday = u32::try_from(self.weekday?).ok()?;
                Some(vec![nth_weekday(
                    year,
                    month,
                    weekday,
                    self.week_of_month?,
                )?])
            }
            _ => None,
        }
    }
}

/// Iterator over a recurrence's nominal dates; see
/// [`Recurrence::dates_from`].
#[derive(Debug, Clone)]
pub struct Dates {
    rec: Recurrence,
    from: Date,
    period: i64,
    /// Dates of the current period still to yield, last first.
    queue: Vec<Date>,
    done: bool,
}

impl Iterator for Dates {
    type Item = Date;

    fn next(&mut self) -> Option<Date> {
        loop {
            if let Some(d) = self.queue.pop() {
                return Some(d);
            }
            if self.done {
                return None;
            }
            let p = self.period;
            self.period += 1;
            let Some(mut dates) = self.rec.period_dates(p) else {
                self.done = true;
                return None;
            };
            if self.rec.frequency == Frequency::Once {
                self.done = true;
            }
            dates.retain(|d| *d >= self.rec.start_date && *d >= self.from);
            dates.reverse();
            self.queue = dates;
        }
    }
}

/// `date` plus `days` (negative goes back); `None` outside the calendar.
pub fn add_days(date: Date, days: i64) -> Option<Date> {
    let delta = TimeDelta::try_days(days)?;
    date.naive().checked_add_signed(delta).map(Date::from_naive)
}

/// The last day of a month.
fn last_day(year: i32, month: u32) -> Option<u32> {
    let (y, m) = if month == 12 {
        (year.checked_add(1)?, 1)
    } else {
        (year, month + 1)
    };
    let first_of_next = NaiveDate::from_ymd_opt(y, m, 1)?;
    first_of_next.pred_opt().map(|d| d.day())
}

/// `day` of the month, or its last day if the month is shorter (REC-040).
fn clamped(year: i32, month: u32, day: u32) -> Option<Date> {
    Date::from_ymd(year, month, day.min(last_day(year, month)?)).ok()
}

/// The `n`th (1–4) or last (−1) `weekday` (1 = Monday) of a month.
fn nth_weekday(year: i32, month: u32, weekday: u32, n: i64) -> Option<Date> {
    if n == -1 {
        let last = last_day(year, month)?;
        let last_wd = NaiveDate::from_ymd_opt(year, month, last)?
            .weekday()
            .number_from_monday();
        let back = (last_wd + 7 - weekday) % 7;
        return Date::from_ymd(year, month, last - back).ok();
    }
    let first_wd = NaiveDate::from_ymd_opt(year, month, 1)?
        .weekday()
        .number_from_monday();
    let first = 1 + (weekday + 7 - first_wd) % 7;
    let day = first + 7 * u32::try_from(n.checked_sub(1)?).ok()?;
    Date::from_ymd(year, month, day).ok()
}

fn ordinal(n: i64) -> String {
    let suffix = match (n % 100, n % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

fn ordinal_word(n: i64) -> &'static str {
    match n {
        1 => "first",
        2 => "second",
        3 => "third",
        4 => "fourth",
        _ => "",
    }
}

fn weekday_name(n: i64) -> &'static str {
    match n {
        1 => "Monday",
        2 => "Tuesday",
        3 => "Wednesday",
        4 => "Thursday",
        5 => "Friday",
        6 => "Saturday",
        _ => "Sunday",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    fn take(rec: &Recurrence, from: &str, n: usize) -> Vec<String> {
        rec.dates_from(d(from))
            .take(n)
            .map(|x| x.to_string())
            .collect()
    }

    fn monthly(day: i64, start: &str) -> Recurrence {
        Recurrence {
            day1: Some(day),
            ..Recurrence::new(Frequency::Monthly, d(start))
        }
    }

    #[test]
    fn once_yields_only_the_start() {
        let r = Recurrence::new(Frequency::Once, d("2026-03-10"));
        assert_eq!(take(&r, "2026-01-01", 5), ["2026-03-10"]);
        assert!(take(&r, "2026-03-11", 5).is_empty());
    }

    #[test]
    fn daily_and_weekly_step_from_start() {
        let mut r = Recurrence::new(Frequency::Daily, d("2026-02-27"));
        r.interval = 2;
        assert_eq!(
            take(&r, "2026-02-27", 4),
            ["2026-02-27", "2026-03-01", "2026-03-03", "2026-03-05"]
        );
        let mut w = Recurrence::new(Frequency::Weekly, d("2026-01-02"));
        w.interval = 2;
        assert_eq!(
            take(&w, "2026-01-10", 3),
            ["2026-01-16", "2026-01-30", "2026-02-13"]
        );
    }

    #[test]
    fn monthly_clamps_to_month_end() {
        let r = monthly(31, "2026-01-31");
        assert_eq!(
            take(&r, "2026-01-01", 5),
            [
                "2026-01-31",
                "2026-02-28",
                "2026-03-31",
                "2026-04-30",
                "2026-05-31"
            ]
        );
    }

    #[test]
    fn monthly_clamp_does_not_drift() {
        // Feb 28 must not pull later months back to the 28th.
        let r = monthly(30, "2026-01-30");
        assert_eq!(
            take(&r, "2026-02-01", 3),
            ["2026-02-28", "2026-03-30", "2026-04-30"]
        );
    }

    #[test]
    fn leap_years() {
        let r = monthly(29, "2027-12-29");
        assert_eq!(
            take(&r, "2027-12-29", 4),
            ["2027-12-29", "2028-01-29", "2028-02-29", "2028-03-29"]
        );
        let r = monthly(29, "2026-12-29");
        assert_eq!(take(&r, "2027-02-01", 1), ["2027-02-28"]);
        let y = Recurrence::new(Frequency::Yearly, d("2024-02-29"));
        assert_eq!(
            take(&y, "2024-02-29", 5),
            [
                "2024-02-29",
                "2025-02-28",
                "2026-02-28",
                "2027-02-28",
                "2028-02-29"
            ]
        );
    }

    #[test]
    fn monthly_starts_at_first_match_on_or_after_start() {
        // Start Jan 20, day 15: the first occurrence is Feb 15.
        let r = monthly(15, "2026-01-20");
        assert_eq!(take(&r, "2026-01-01", 2), ["2026-02-15", "2026-03-15"]);
    }

    #[test]
    fn quarterly_and_twice_a_year_are_monthly_intervals() {
        let mut q = monthly(15, "2026-01-15");
        q.interval = 3;
        assert_eq!(
            take(&q, "2026-01-01", 4),
            ["2026-01-15", "2026-04-15", "2026-07-15", "2026-10-15"]
        );
        let mut h = monthly(1, "2026-03-01");
        h.interval = 6;
        assert_eq!(
            take(&h, "2026-01-01", 3),
            ["2026-03-01", "2026-09-01", "2027-03-01"]
        );
    }

    #[test]
    fn twice_monthly() {
        let mut r = Recurrence::new(Frequency::TwiceMonthly, d("2026-01-01"));
        r.day1 = Some(1);
        r.day2 = Some(15);
        assert_eq!(
            take(&r, "2026-01-10", 4),
            ["2026-01-15", "2026-02-01", "2026-02-15", "2026-03-01"]
        );
    }

    #[test]
    fn twice_monthly_clamps_and_never_repeats_a_date() {
        let mut r = Recurrence::new(Frequency::TwiceMonthly, d("2026-01-01"));
        r.day1 = Some(15);
        r.day2 = Some(31);
        assert_eq!(
            take(&r, "2026-02-01", 4),
            ["2026-02-15", "2026-02-28", "2026-03-15", "2026-03-31"]
        );
        r.day1 = Some(30);
        assert_eq!(
            take(&r, "2026-02-01", 3),
            ["2026-02-28", "2026-03-30", "2026-03-31"]
        );
    }

    #[test]
    fn last_day_of_month() {
        let mut r = Recurrence::new(Frequency::MonthlyLastDay, d("2027-12-15"));
        assert_eq!(
            take(&r, "2027-12-15", 4),
            ["2027-12-31", "2028-01-31", "2028-02-29", "2028-03-31"]
        );
        r.interval = 2;
        assert_eq!(
            take(&r, "2027-12-15", 3),
            ["2027-12-31", "2028-02-29", "2028-04-30"]
        );
    }

    #[test]
    fn nth_weekday_of_month() {
        // Second Tuesday.
        let mut r = Recurrence::new(Frequency::MonthlyNthWeekday, d("2026-01-01"));
        r.weekday = Some(2);
        r.week_of_month = Some(2);
        assert_eq!(
            take(&r, "2026-01-01", 4),
            ["2026-01-13", "2026-02-10", "2026-03-10", "2026-04-14"]
        );
        // Last Friday.
        r.weekday = Some(5);
        r.week_of_month = Some(-1);
        assert_eq!(
            take(&r, "2026-01-01", 4),
            ["2026-01-30", "2026-02-27", "2026-03-27", "2026-04-24"]
        );
        // First Sunday of a month that starts on a Sunday (Feb 2026).
        r.weekday = Some(7);
        r.week_of_month = Some(1);
        assert_eq!(take(&r, "2026-02-01", 1), ["2026-02-01"]);
        // Fourth Thursday, every 3 months.
        r.weekday = Some(4);
        r.week_of_month = Some(4);
        r.interval = 3;
        assert_eq!(
            take(&r, "2026-01-01", 3),
            ["2026-01-22", "2026-04-23", "2026-07-23"]
        );
    }

    #[test]
    fn far_from_dates_jump_ahead_without_skipping() {
        let r = monthly(31, "2000-01-31");
        assert_eq!(take(&r, "2026-02-01", 2), ["2026-02-28", "2026-03-31"]);
        let mut w = Recurrence::new(Frequency::Weekly, d("2000-01-03"));
        w.interval = 3;
        let first = w.first_on_or_after(d("2026-06-30")).unwrap();
        assert!(first >= d("2026-06-30"));
        assert!(add_days(first, -21).unwrap() < d("2026-06-30"));
        assert!(w.contains(first));
    }

    #[test]
    fn weekend_rule_moves_saturday_and_sunday() {
        let mut r = monthly(1, "2026-01-01");
        // 2026-08-01 is a Saturday, 2026-11-01 a Sunday.
        assert_eq!(r.due_date(d("2026-08-01")), d("2026-08-01"));
        r.weekend_rule = WeekendRule::Previous;
        assert_eq!(r.due_date(d("2026-08-01")), d("2026-07-31"));
        assert_eq!(r.due_date(d("2026-11-01")), d("2026-10-30"));
        r.weekend_rule = WeekendRule::Next;
        assert_eq!(r.due_date(d("2026-08-01")), d("2026-08-03"));
        assert_eq!(r.due_date(d("2026-11-01")), d("2026-11-02"));
        assert_eq!(r.due_date(d("2026-08-03")), d("2026-08-03"));
    }

    #[test]
    fn validate_rejects_bad_combinations() {
        let ok = monthly(15, "2026-01-01");
        assert!(ok.validate().is_ok());
        let mut r = ok.clone();
        r.interval = 0;
        assert!(r.validate().is_err());
        let mut r = ok.clone();
        r.day1 = None;
        assert!(r.validate().is_err());
        let mut r = ok.clone();
        r.weekday = Some(1);
        assert!(r.validate().is_err());
        let mut t = Recurrence::new(Frequency::TwiceMonthly, d("2026-01-01"));
        t.day1 = Some(15);
        t.day2 = Some(15);
        assert!(t.validate().is_err());
        t.day2 = Some(20);
        assert!(t.validate().is_ok());
        t.interval = 2;
        assert!(t.validate().is_err());
        let mut n = Recurrence::new(Frequency::MonthlyNthWeekday, d("2026-01-01"));
        assert!(n.validate().is_err());
        n.weekday = Some(3);
        n.week_of_month = Some(5);
        assert!(n.validate().is_err());
        n.week_of_month = Some(-1);
        assert!(n.validate().is_ok());
    }

    #[test]
    fn descriptions() {
        assert_eq!(monthly(1, "2026-01-01").describe(), "Monthly on the 1st");
        let mut q = monthly(22, "2026-01-01");
        q.interval = 3;
        assert_eq!(q.describe(), "Quarterly on the 22nd");
        let mut n = Recurrence::new(Frequency::MonthlyNthWeekday, d("2026-01-01"));
        n.weekday = Some(2);
        n.week_of_month = Some(2);
        assert_eq!(n.describe(), "The second Tuesday of every month");
        let mut w = Recurrence::new(Frequency::Weekly, d("2026-01-02"));
        w.interval = 2;
        assert_eq!(w.describe(), "Every 2 weeks on Friday");
    }
}
