//! The Graph View's periods (BUD-060): which months each covers, its
//! name, and the periods before and after it.

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::error::{Error, Result};
use crate::text_enum::text_enum;

text_enum! {
    /// The Graph View's second control (BUD-060).
    pub enum GraphPeriod {
        Monthly = "monthly",
        Quarterly = "quarterly",
        Yearly = "yearly",
        QuarterToDate = "quarter_to_date",
        YearToDate = "year_to_date",
        Last3Months = "last_3_months",
        Last6Months = "last_6_months",
    }
}

/// Which month, quarter, or year a Monthly, Quarterly, or Yearly period
/// shows: `index` is the month (1–12), the quarter (1–4), or 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct GraphAt {
    pub year: i32,
    pub index: u32,
}

/// A year and month (1–12).
pub(crate) type Ym = (i32, u32);

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

const LONG_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// `ym` moved by `n` months.
pub(crate) fn add_months((y, m): Ym, n: i32) -> Ym {
    let i = y * 12 + i32::try_from(m).unwrap_or(1) - 1 + n;
    (
        i.div_euclid(12),
        u32::try_from(i.rem_euclid(12)).unwrap_or(0) + 1,
    )
}

/// "Oct 2026".
pub(crate) fn month_label((y, m): Ym) -> String {
    let name = MONTHS
        .get(usize::try_from(m).unwrap_or(0).wrapping_sub(1))
        .copied()
        .unwrap_or("");
    format!("{name} {y}")
}

/// "October".
pub(crate) fn long_month(m: u32) -> &'static str {
    LONG_MONTHS
        .get(usize::try_from(m).unwrap_or(0).wrapping_sub(1))
        .copied()
        .unwrap_or("")
}

/// "Jan 2026 - Oct 2026", or "Oct 2026" for one month.
pub(crate) fn range_label(months: &[Ym]) -> String {
    match (months.first(), months.last()) {
        (Some(a), Some(b)) if a == b => month_label(*a),
        (Some(a), Some(b)) => format!("{} - {}", month_label(*a), month_label(*b)),
        _ => String::new(),
    }
}

/// A resolved period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Span {
    /// Its months, in order.
    pub months: Vec<Ym>,
    /// The month, quarter, or year shown (Monthly, Quarterly, Yearly).
    pub at: Option<GraphAt>,
    pub prev: Option<GraphAt>,
    pub next: Option<GraphAt>,
    /// "October 2026", "Q4 2026", "2026"; empty for the others.
    pub title: String,
    /// A month in a year the budget lacks asks to create it (BUD-050);
    /// Last 3 and Last 6 months count it as 0 instead.
    pub asks: bool,
}

fn run(start: Ym, count: i32) -> Vec<Ym> {
    (0..count).map(|i| add_months(start, i)).collect()
}

fn bad(at: GraphAt) -> Error {
    Error::Invalid(format!(
        "no such period: year {}, index {}",
        at.year, at.index
    ))
}

/// The months `period` covers, `at` its month, quarter, or year (the
/// current one when `None`).
pub(crate) fn resolve(period: GraphPeriod, at: Option<GraphAt>, today: Date) -> Result<Span> {
    let now: Ym = (today.year(), today.month());
    let quarter = |m: u32| (m - 1) / 3 + 1;
    let plain = |months: Vec<Ym>, asks| Span {
        months,
        at: None,
        prev: None,
        next: None,
        title: String::new(),
        asks,
    };
    Ok(match period {
        GraphPeriod::Monthly => {
            let at = at.unwrap_or(GraphAt {
                year: now.0,
                index: now.1,
            });
            if !(1..=12).contains(&at.index) {
                return Err(bad(at));
            }
            let ym = (at.year, at.index);
            let step = |n| {
                let (year, index) = add_months(ym, n);
                GraphAt { year, index }
            };
            Span {
                months: vec![ym],
                at: Some(at),
                prev: Some(step(-1)),
                next: Some(step(1)),
                title: format!("{} {}", long_month(at.index), at.year),
                asks: true,
            }
        }
        GraphPeriod::Quarterly => {
            let at = at.unwrap_or(GraphAt {
                year: now.0,
                index: quarter(now.1),
            });
            if !(1..=4).contains(&at.index) {
                return Err(bad(at));
            }
            let step = |n: i32| {
                let i = at.year * 4 + i32::try_from(at.index).unwrap_or(1) - 1 + n;
                GraphAt {
                    year: i.div_euclid(4),
                    index: u32::try_from(i.rem_euclid(4)).unwrap_or(0) + 1,
                }
            };
            Span {
                months: run((at.year, (at.index - 1) * 3 + 1), 3),
                at: Some(at),
                prev: Some(step(-1)),
                next: Some(step(1)),
                title: format!("Q{} {}", at.index, at.year),
                asks: true,
            }
        }
        GraphPeriod::Yearly => {
            let at = at.unwrap_or(GraphAt {
                year: now.0,
                index: 1,
            });
            let year = |year| GraphAt { year, index: 1 };
            Span {
                months: run((at.year, 1), 12),
                at: Some(year(at.year)),
                prev: Some(year(at.year - 1)),
                next: Some(year(at.year + 1)),
                title: at.year.to_string(),
                asks: true,
            }
        }
        GraphPeriod::QuarterToDate => {
            let first = (quarter(now.1) - 1) * 3 + 1;
            plain(
                run(
                    (now.0, first),
                    i32::try_from(now.1 - first + 1).unwrap_or(1),
                ),
                true,
            )
        }
        GraphPeriod::YearToDate => plain(run((now.0, 1), i32::try_from(now.1).unwrap_or(1)), true),
        GraphPeriod::Last3Months => plain(run(add_months(now, -3), 3), false),
        GraphPeriod::Last6Months => plain(run(add_months(now, -6), 6), false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    fn at(year: i32, index: u32) -> Option<GraphAt> {
        Some(GraphAt { year, index })
    }

    #[test]
    fn months_wrap_across_years() {
        assert_eq!(add_months((2026, 1), -1), (2025, 12));
        assert_eq!(add_months((2026, 12), 1), (2027, 1));
        assert_eq!(add_months((2026, 3), -15), (2024, 12));
    }

    #[test]
    fn current_periods_start_from_today() {
        let today = d("2026-10-13");
        let m = resolve(GraphPeriod::Monthly, None, today).unwrap();
        assert_eq!(m.months, vec![(2026, 10)]);
        assert_eq!(m.title, "October 2026");
        assert_eq!(m.prev, at(2026, 9));
        assert_eq!(m.next, at(2026, 11));
        let q = resolve(GraphPeriod::Quarterly, None, today).unwrap();
        assert_eq!(q.months, vec![(2026, 10), (2026, 11), (2026, 12)]);
        assert_eq!(q.title, "Q4 2026");
        assert_eq!(q.next, at(2027, 1));
        let y = resolve(GraphPeriod::Yearly, None, today).unwrap();
        assert_eq!(y.months.len(), 12);
        assert_eq!(y.prev, at(2025, 1));
        let qtd = resolve(GraphPeriod::QuarterToDate, None, today).unwrap();
        assert_eq!(qtd.months, vec![(2026, 10)]);
        let ytd = resolve(GraphPeriod::YearToDate, None, today).unwrap();
        assert_eq!(range_label(&ytd.months), "Jan 2026 - Oct 2026");
        assert!(ytd.at.is_none() && ytd.asks);
    }

    #[test]
    fn last_months_are_full_months_before_this_one_and_never_ask() {
        let today = d("2026-02-10");
        let l3 = resolve(GraphPeriod::Last3Months, None, today).unwrap();
        assert_eq!(l3.months, vec![(2025, 11), (2025, 12), (2026, 1)]);
        assert!(!l3.asks);
        let l6 = resolve(GraphPeriod::Last6Months, None, today).unwrap();
        assert_eq!(range_label(&l6.months), "Aug 2025 - Jan 2026");
    }

    #[test]
    fn a_chosen_period_steps_back_and_forward() {
        let today = d("2026-10-13");
        let m = resolve(GraphPeriod::Monthly, at(2027, 1), today).unwrap();
        assert_eq!(m.prev, at(2026, 12));
        let q = resolve(GraphPeriod::Quarterly, at(2026, 1), today).unwrap();
        assert_eq!(q.prev, at(2025, 4));
        assert!(resolve(GraphPeriod::Monthly, at(2026, 13), today).is_err());
        assert!(resolve(GraphPeriod::Quarterly, at(2026, 0), today).is_err());
    }
}
