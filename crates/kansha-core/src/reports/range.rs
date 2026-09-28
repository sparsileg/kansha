//! Date range presets (RPT-040) and report periods.

use chrono::{Duration, Months, NaiveDate};
use rusqlite::Connection;

use super::{DatePreset, DateRange, Interval, ResolvedRange};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::persistence::reports as repo;

fn ymd(y: i32, m: u32, d: u32) -> Result<Date> {
    Date::from_ymd(y, m, d)
}

fn add_days(d: Date, n: i64) -> Result<Date> {
    d.naive()
        .checked_add_signed(Duration::days(n))
        .map(Date::from_naive)
        .ok_or(Error::Overflow("date"))
}

fn add_months(d: NaiveDate, n: u32) -> Result<NaiveDate> {
    d.checked_add_months(Months::new(n))
        .ok_or(Error::Overflow("date"))
}

fn sub_months(d: NaiveDate, n: u32) -> Result<NaiveDate> {
    d.checked_sub_months(Months::new(n))
        .ok_or(Error::Overflow("date"))
}

/// Last day of the month `months` after `d`'s month (0 = its own).
fn month_end(d: Date, months: u32) -> Result<Date> {
    let first = ymd(d.year(), d.month(), 1)?.naive();
    let next = add_months(first, months + 1)?;
    Ok(Date::from_naive(
        next.pred_opt().ok_or(Error::Overflow("date"))?,
    ))
}

fn quarter_start(d: Date) -> Result<Date> {
    ymd(d.year(), (d.month() - 1) / 3 * 3 + 1, 1)
}

/// A range's dates on `today`. "All dates" starts at the first
/// transaction; a custom range with no start does too.
pub fn resolve(conn: &Connection, range: &DateRange, today: Date) -> Result<ResolvedRange> {
    use DatePreset as P;
    let year_start = ymd(today.year(), 1, 1)?;
    let month_start = ymd(today.year(), today.month(), 1)?;
    let (from, to) = match range.preset {
        P::AllDates => (repo::first_txn_date(conn)?, today),
        P::MonthToDate => (Some(month_start), today),
        P::QuarterToDate => (Some(quarter_start(today)?), today),
        P::YearToDate => (Some(year_start), today),
        P::ThisMonth => (Some(month_start), month_end(today, 0)?),
        P::LastMonth => {
            let start = Date::from_naive(sub_months(month_start.naive(), 1)?);
            (Some(start), month_end(start, 0)?)
        }
        P::ThisQuarter => {
            let start = quarter_start(today)?;
            (Some(start), month_end(start, 2)?)
        }
        P::LastQuarter => {
            let start = Date::from_naive(sub_months(quarter_start(today)?.naive(), 3)?);
            (Some(start), month_end(start, 2)?)
        }
        P::ThisYear => (Some(year_start), ymd(today.year(), 12, 31)?),
        P::LastYear => (
            Some(ymd(today.year() - 1, 1, 1)?),
            ymd(today.year() - 1, 12, 31)?,
        ),
        P::Last30Days => (Some(add_days(today, -29)?), today),
        P::Last12Months => {
            let start = Date::from_naive(sub_months(add_days(today, 1)?.naive(), 12)?);
            (Some(start), today)
        }
        P::Custom => {
            let to = range.to.unwrap_or(today);
            let from = match range.from {
                Some(f) => Some(f),
                None => repo::first_txn_date(conn)?,
            };
            (from, to)
        }
    };
    if from.is_some_and(|f| f > to) {
        return Err(Error::Invalid(
            "the report's start date is after its end date".into(),
        ));
    }
    Ok(ResolvedRange { from, to })
}

/// Split `from..=to` into periods. Months, quarters, half-years, and
/// years follow the calendar (the first and last are cut to the range);
/// weeks and two-week periods count from `from`; half-months are the 1st
/// to the 15th and the 16th to the month's end. `None` is one period.
pub fn periods(from: Date, to: Date, interval: Interval) -> Result<Vec<(Date, Date)>> {
    let mut out = Vec::new();
    let mut start = from;
    while start <= to {
        let natural_end = match interval {
            Interval::None => to,
            Interval::Week => add_days(start, 6)?,
            Interval::TwoWeeks => add_days(start, 13)?,
            Interval::HalfMonth => {
                if start.day() <= 15 {
                    ymd(start.year(), start.month(), 15)?
                } else {
                    month_end(start, 0)?
                }
            }
            Interval::Month => month_end(start, 0)?,
            Interval::Quarter => month_end(quarter_start(start)?, 2)?,
            Interval::HalfYear => {
                let first = if start.month() <= 6 { 1 } else { 7 };
                month_end(ymd(start.year(), first, 1)?, 5)?
            }
            Interval::Year => ymd(start.year(), 12, 31)?,
        };
        let end = natural_end.min(to);
        out.push((start, end));
        start = add_days(end, 1)?;
    }
    Ok(out)
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A period column's heading: "Jan 2026", "Q1 2026", "H1 2026", "2026".
/// Empty for weeks and half-months: the table shows their dates, in the
/// user's date format.
pub fn period_label(start: Date, interval: Interval) -> String {
    let month = |d: Date| MONTHS[(d.month() as usize).saturating_sub(1) % 12];
    match interval {
        Interval::Month => format!("{} {}", month(start), start.year()),
        Interval::Quarter => format!("Q{} {}", (start.month() - 1) / 3 + 1, start.year()),
        Interval::HalfYear => format!(
            "H{} {}",
            if start.month() <= 6 { 1 } else { 2 },
            start.year()
        ),
        Interval::Year => start.year().to_string(),
        Interval::None | Interval::Week | Interval::TwoWeeks | Interval::HalfMonth => String::new(),
    }
}

/// The day before `d`.
pub(super) fn day_before(d: Date) -> Result<Date> {
    add_days(d, -1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FixedClock;
    use crate::persistence::Db;

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    fn range(preset: DatePreset) -> DateRange {
        DateRange {
            preset,
            from: None,
            to: None,
        }
    }

    #[test]
    fn presets_resolve_against_today() {
        let today = d("2026-09-27");
        let db = Db::open_in_memory(&FixedClock::new(today)).unwrap();
        let r = |p| resolve(db.conn(), &range(p), today).unwrap();
        let pair = |p| {
            let x = r(p);
            (x.from.unwrap().to_string(), x.to.to_string())
        };
        assert_eq!(
            pair(DatePreset::YearToDate),
            ("2026-01-01".into(), "2026-09-27".into())
        );
        assert_eq!(
            pair(DatePreset::LastYear),
            ("2025-01-01".into(), "2025-12-31".into())
        );
        assert_eq!(
            pair(DatePreset::LastMonth),
            ("2026-08-01".into(), "2026-08-31".into())
        );
        assert_eq!(
            pair(DatePreset::ThisQuarter),
            ("2026-07-01".into(), "2026-09-30".into())
        );
        assert_eq!(
            pair(DatePreset::LastQuarter),
            ("2026-04-01".into(), "2026-06-30".into())
        );
        assert_eq!(
            pair(DatePreset::QuarterToDate),
            ("2026-07-01".into(), "2026-09-27".into())
        );
        assert_eq!(
            pair(DatePreset::Last30Days),
            ("2026-08-29".into(), "2026-09-27".into())
        );
        assert_eq!(
            pair(DatePreset::Last12Months),
            ("2025-09-28".into(), "2026-09-27".into())
        );
        assert_eq!(
            pair(DatePreset::ThisMonth),
            ("2026-09-01".into(), "2026-09-30".into())
        );
        // No transactions yet: all dates has no start.
        assert_eq!(r(DatePreset::AllDates).from, None);
    }

    #[test]
    fn last_month_in_january_is_december() {
        let today = d("2026-01-10");
        let db = Db::open_in_memory(&FixedClock::new(today)).unwrap();
        let x = resolve(db.conn(), &range(DatePreset::LastMonth), today).unwrap();
        assert_eq!(x.from, Some(d("2025-12-01")));
        assert_eq!(x.to, d("2025-12-31"));
    }

    #[test]
    fn custom_range_must_be_in_order() {
        let today = d("2026-09-27");
        let db = Db::open_in_memory(&FixedClock::new(today)).unwrap();
        let bad = DateRange {
            preset: DatePreset::Custom,
            from: Some(d("2026-05-01")),
            to: Some(d("2026-04-01")),
        };
        assert!(resolve(db.conn(), &bad, today).is_err());
    }

    #[test]
    fn periods_follow_the_calendar_and_cut_to_the_range() {
        let p = periods(d("2026-01-15"), d("2026-04-10"), Interval::Month).unwrap();
        let text: Vec<_> = p.iter().map(|(a, b)| format!("{a}..{b}")).collect();
        assert_eq!(
            text,
            [
                "2026-01-15..2026-01-31",
                "2026-02-01..2026-02-28",
                "2026-03-01..2026-03-31",
                "2026-04-01..2026-04-10"
            ]
        );
        let q = periods(d("2026-02-01"), d("2026-12-31"), Interval::Quarter).unwrap();
        assert_eq!(q.len(), 4);
        assert_eq!(q[0], (d("2026-02-01"), d("2026-03-31")));
        let h = periods(d("2026-01-01"), d("2026-01-31"), Interval::HalfMonth).unwrap();
        assert_eq!(
            h,
            [
                (d("2026-01-01"), d("2026-01-15")),
                (d("2026-01-16"), d("2026-01-31"))
            ]
        );
        let w = periods(d("2026-01-01"), d("2026-01-10"), Interval::Week).unwrap();
        assert_eq!(
            w,
            [
                (d("2026-01-01"), d("2026-01-07")),
                (d("2026-01-08"), d("2026-01-10"))
            ]
        );
        let one = periods(d("2026-01-01"), d("2026-01-10"), Interval::None).unwrap();
        assert_eq!(one, [(d("2026-01-01"), d("2026-01-10"))]);
    }

    #[test]
    fn period_labels() {
        assert_eq!(period_label(d("2026-03-01"), Interval::Month), "Mar 2026");
        assert_eq!(period_label(d("2026-08-01"), Interval::Quarter), "Q3 2026");
        assert_eq!(period_label(d("2026-08-01"), Interval::HalfYear), "H2 2026");
        assert_eq!(period_label(d("2026-08-01"), Interval::Year), "2026");
        assert_eq!(period_label(d("2026-08-01"), Interval::Week), "");
    }
}
