//! Date range presets (RPT-040) and report periods.

use chrono::{Datelike, Duration, Months, NaiveDate};
use rusqlite::Connection;

use super::{CompareTo, DatePreset, DateRange, Interval, PeriodChoice, ResolvedRange};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::persistence::reports as repo;
use crate::settings::{self, WeekStart};

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
pub(super) fn month_end(d: Date, months: u32) -> Result<Date> {
    let first = ymd(d.year(), d.month(), 1)?.naive();
    let next = add_months(first, months + 1)?;
    Ok(Date::from_naive(
        next.pred_opt().ok_or(Error::Overflow("date"))?,
    ))
}

fn quarter_start(d: Date) -> Result<Date> {
    ymd(d.year(), (d.month() - 1) / 3 * 3 + 1, 1)
}

/// The first day of the week holding `d`.
fn week_start(d: Date, first: WeekStart) -> Result<Date> {
    let wd = d.naive().weekday();
    let back = match first {
        WeekStart::Sunday => wd.num_days_from_sunday(),
        WeekStart::Monday => wd.num_days_from_monday(),
    };
    add_days(d, -i64::from(back))
}

/// The periods a comparison report averages (RPT-210): `compare`'s
/// weeks, months, quarters, or years ending on `to`, so a range's own
/// dates are part of its average.
pub fn compare_window(to: Date, compare: CompareTo) -> Result<(Date, Date)> {
    use CompareTo as C;
    let n = compare.count();
    let after = add_days(to, 1)?.naive();
    let from = match compare {
        C::Weeks4 | C::Weeks8 | C::Weeks12 => return Ok((add_days(to, 1 - 7 * i64::from(n))?, to)),
        C::Months3 | C::Months6 | C::Months12 => sub_months(after, n)?,
        C::Quarters4 | C::Quarters8 | C::Quarters12 => sub_months(after, 3 * n)?,
        C::Years1 | C::Years3 | C::Years5 => sub_months(after, 12 * n)?,
    };
    Ok((Date::from_naive(from), to))
}

/// The periods a comparison report averages for Custom dates
/// (RPT-210): `compare`'s whole weeks, months, quarters, or years
/// before the current one on `today` ("last year" in 2026 is 2025).
pub fn compare_before(today: Date, compare: CompareTo, first: WeekStart) -> Result<(Date, Date)> {
    use CompareTo as C;
    let n = compare.count();
    let (start, from) = match compare {
        C::Weeks4 | C::Weeks8 | C::Weeks12 => {
            let start = week_start(today, first)?;
            (start, add_days(start, -7 * i64::from(n))?)
        }
        C::Months3 | C::Months6 | C::Months12 => {
            let start = ymd(today.year(), today.month(), 1)?;
            (start, Date::from_naive(sub_months(start.naive(), n)?))
        }
        C::Quarters4 | C::Quarters8 | C::Quarters12 => {
            let start = quarter_start(today)?;
            (start, Date::from_naive(sub_months(start.naive(), 3 * n)?))
        }
        C::Years1 | C::Years3 | C::Years5 => {
            let start = ymd(today.year(), 1, 1)?;
            (start, Date::from_naive(sub_months(start.naive(), 12 * n)?))
        }
    };
    Ok((from, add_days(start, -1)?))
}

/// A range's dates on `today`. "All dates" starts at the first
/// transaction; a custom range with no start does too.
pub fn resolve(conn: &Connection, range: &DateRange, today: Date) -> Result<ResolvedRange> {
    use DatePreset as P;
    let year_start = ymd(today.year(), 1, 1)?;
    let month_start = ymd(today.year(), today.month(), 1)?;
    let (from, to) = match range.preset {
        P::AllDates => (repo::first_txn_date(conn)?, today),
        P::Monthly | P::Quarterly | P::Yearly => {
            let (start, end) = period_of(range.preset, range.from.unwrap_or(today))?;
            (Some(start), end)
        }
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
        P::WeekToDate => {
            let first = settings::load(conn)?.week_start;
            (Some(week_start(today, first)?), today)
        }
        P::LastWeek => {
            let first = settings::load(conn)?.week_start;
            let start = add_days(week_start(today, first)?, -7)?;
            (Some(start), add_days(start, 6)?)
        }
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

/// The calendar month, quarter, or year (by `preset`) holding `d`.
fn period_of(preset: DatePreset, d: Date) -> Result<(Date, Date)> {
    match preset {
        DatePreset::Monthly => {
            let start = ymd(d.year(), d.month(), 1)?;
            Ok((start, month_end(start, 0)?))
        }
        DatePreset::Quarterly => {
            let start = quarter_start(d)?;
            Ok((start, month_end(start, 2)?))
        }
        _ => Ok((ymd(d.year(), 1, 1)?, ymd(d.year(), 12, 31)?)),
    }
}

fn period_interval(preset: DatePreset) -> Interval {
    match preset {
        DatePreset::Monthly => Interval::Month,
        DatePreset::Quarterly => Interval::Quarter,
        _ => Interval::Year,
    }
}

/// The periods offered for Monthly, Quarterly, or Yearly, newest first:
/// this month back to the same month last year (13), this quarter back
/// to the same quarter last year (5), or the last five years with this
/// one. The period `range.from` picks is added at the end when it is
/// older (a saved report). Other presets have none.
pub fn period_choices(range: &DateRange, today: Date) -> Result<Vec<PeriodChoice>> {
    let (count, step): (u32, u32) = match range.preset {
        DatePreset::Monthly => (13, 1),
        DatePreset::Quarterly => (5, 3),
        DatePreset::Yearly => (5, 12),
        _ => return Ok(Vec::new()),
    };
    let interval = period_interval(range.preset);
    let choice = |d: Date| -> Result<PeriodChoice> {
        let (from, to) = period_of(range.preset, d)?;
        Ok(PeriodChoice {
            from,
            to,
            label: period_label(from, interval),
        })
    };
    let (current, _) = period_of(range.preset, today)?;
    let mut out = Vec::new();
    for i in 0..count {
        let d = Date::from_naive(sub_months(current.naive(), i * step)?);
        out.push(choice(d)?);
    }
    if let Some(f) = range.from {
        let picked = choice(f)?;
        if !out.iter().any(|c| c.from == picked.from) {
            out.push(picked);
        }
    }
    Ok(out)
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

    /// Week presets follow Settings' first day of the week (RPT-210).
    #[test]
    fn week_presets_start_on_the_chosen_day() {
        // A Thursday.
        let today = d("2026-10-01");
        let clock = FixedClock::new(today);
        let mut db = Db::open_in_memory(&clock).unwrap();
        let pair = |db: &Db, p| {
            let x = resolve(db.conn(), &range(p), today).unwrap();
            (x.from.unwrap().to_string(), x.to.to_string())
        };
        let s = |a: &str, b: &str| (a.to_string(), b.to_string());
        assert_eq!(
            pair(&db, DatePreset::WeekToDate),
            s("2026-09-27", "2026-10-01")
        );
        assert_eq!(
            pair(&db, DatePreset::LastWeek),
            s("2026-09-20", "2026-09-26")
        );
        let mut set = settings::load(db.conn()).unwrap();
        set.week_start = WeekStart::Monday;
        db.write(&clock, crate::Origin::System, |tx| settings::save(tx, &set))
            .unwrap();
        assert_eq!(
            pair(&db, DatePreset::WeekToDate),
            s("2026-09-28", "2026-10-01")
        );
        assert_eq!(
            pair(&db, DatePreset::LastWeek),
            s("2026-09-21", "2026-09-27")
        );
    }

    /// The averaged periods end on the range's last day (RPT-210).
    #[test]
    fn compare_window_ends_on_the_last_day() {
        let w = |to: &str, c| {
            let (a, b) = compare_window(d(to), c).unwrap();
            format!("{a} {b}")
        };
        assert_eq!(w("2026-09-30", CompareTo::Months3), "2026-07-01 2026-09-30");
        assert_eq!(w("2026-10-05", CompareTo::Months3), "2026-07-06 2026-10-05");
        assert_eq!(w("2026-02-28", CompareTo::Months3), "2025-12-01 2026-02-28");
        assert_eq!(w("2026-10-05", CompareTo::Weeks4), "2026-09-08 2026-10-05");
        assert_eq!(
            w("2026-06-30", CompareTo::Quarters4),
            "2025-07-01 2026-06-30"
        );
        assert_eq!(w("2026-12-31", CompareTo::Years1), "2026-01-01 2026-12-31");
        assert_eq!(w("2026-10-05", CompareTo::Years5), "2021-10-06 2026-10-05");
    }

    /// Custom dates: whole periods before today's (RPT-210).
    #[test]
    fn compare_before_counts_back_from_today() {
        // A Thursday.
        let today = d("2026-10-01");
        let w = |c, first| {
            let (a, b) = compare_before(today, c, first).unwrap();
            format!("{a} {b}")
        };
        let sun = WeekStart::Sunday;
        assert_eq!(w(CompareTo::Years1, sun), "2025-01-01 2025-12-31");
        assert_eq!(w(CompareTo::Years3, sun), "2023-01-01 2025-12-31");
        assert_eq!(w(CompareTo::Months3, sun), "2026-07-01 2026-09-30");
        assert_eq!(w(CompareTo::Quarters4, sun), "2025-10-01 2026-09-30");
        assert_eq!(w(CompareTo::Weeks4, sun), "2026-08-30 2026-09-26");
        assert_eq!(
            w(CompareTo::Weeks4, WeekStart::Monday),
            "2026-08-31 2026-09-27"
        );
    }

    /// Date range presets (RPT-040).
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
    fn period_presets_resolve_to_the_chosen_period_or_todays() {
        let today = d("2026-10-03");
        let db = Db::open_in_memory(&FixedClock::new(today)).unwrap();
        let r = |preset, from: Option<&str>| {
            let x = resolve(
                db.conn(),
                &DateRange {
                    preset,
                    from: from.map(d),
                    to: None,
                },
                today,
            )
            .unwrap();
            format!("{}..{}", x.from.unwrap(), x.to)
        };
        assert_eq!(r(DatePreset::Monthly, None), "2026-10-01..2026-10-31");
        assert_eq!(
            r(DatePreset::Monthly, Some("2026-02-01")),
            "2026-02-01..2026-02-28"
        );
        assert_eq!(r(DatePreset::Quarterly, None), "2026-10-01..2026-12-31");
        assert_eq!(
            r(DatePreset::Quarterly, Some("2025-04-01")),
            "2025-04-01..2025-06-30"
        );
        assert_eq!(r(DatePreset::Yearly, None), "2026-01-01..2026-12-31");
        assert_eq!(
            r(DatePreset::Yearly, Some("2023-01-01")),
            "2023-01-01..2023-12-31"
        );
    }

    #[test]
    fn period_choices_run_back_a_year_or_five_years() {
        let today = d("2026-10-03");
        let labels = |preset, from: Option<&str>| -> Vec<String> {
            let range = DateRange {
                preset,
                from: from.map(d),
                to: None,
            };
            period_choices(&range, today)
                .unwrap()
                .into_iter()
                .map(|c| c.label)
                .collect()
        };
        let months = labels(DatePreset::Monthly, None);
        assert_eq!(months.len(), 13);
        assert_eq!(months[0], "Oct 2026");
        assert_eq!(months[1], "Sep 2026");
        assert_eq!(months[12], "Oct 2025");
        assert_eq!(
            labels(DatePreset::Quarterly, None),
            ["Q4 2026", "Q3 2026", "Q2 2026", "Q1 2026", "Q4 2025"]
        );
        assert_eq!(
            labels(DatePreset::Yearly, None),
            ["2026", "2025", "2024", "2023", "2022"]
        );
        // A saved period off the list is added at the end; one on it is not.
        assert_eq!(
            labels(DatePreset::Yearly, Some("2019-01-01"))
                .last()
                .unwrap(),
            "2019"
        );
        assert_eq!(labels(DatePreset::Yearly, Some("2024-01-01")).len(), 5);
        assert!(labels(DatePreset::LastYear, None).is_empty());
        let q = period_choices(
            &DateRange {
                preset: DatePreset::Quarterly,
                from: None,
                to: None,
            },
            today,
        )
        .unwrap();
        assert_eq!((q[4].from, q[4].to), (d("2025-10-01"), d("2025-12-31")));
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
