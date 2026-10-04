//! Graph data (RPT-010). The engine picks the axis and places every
//! value on it as a position from 0 (bottom) to 10 000 (top), so the
//! frontend only scales positions to pixels; it never does arithmetic on
//! amounts.

use rust_decimal::Decimal;
use serde::Serialize;

use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::{Money, mul_div};
use crate::text_enum::text_enum;

/// The top of the position scale.
pub const SCALE: i64 = 10_000;

text_enum! {
    /// How a series is drawn.
    pub enum SeriesStyle {
        Bar = "bar",
        Line = "line",
    }
}

/// One series: a value per date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Series {
    pub name: String,
    pub style: SeriesStyle,
    pub values: Vec<Money>,
    /// Each value's position, 0 ..= 10 000.
    pub pos: Vec<i64>,
}

text_enum! {
    /// What the date axis names: each day (a span of three months or
    /// less), each month, or each year (a span over three years, or points
    /// most of a year apart).
    pub enum XUnit {
        Day = "day",
        Month = "month",
        Year = "year",
    }
}

/// Days up to which the axis names days.
const DAY_SPAN: i64 = 92;
/// Days beyond which the axis names years.
const YEAR_SPAN: i64 = 1100;
/// Average days between points from which the axis names years.
const YEAR_GAP: i64 = 300;

/// The axis unit for `dates` (oldest first).
pub fn x_unit(dates: &[Date]) -> XUnit {
    let (Some(first), Some(last)) = (dates.first(), dates.last()) else {
        return XUnit::Month;
    };
    let span = (last.naive() - first.naive()).num_days();
    let gaps = i64::try_from(dates.len().saturating_sub(1))
        .unwrap_or(i64::MAX)
        .max(1);
    if span <= DAY_SPAN {
        XUnit::Day
    } else if span > YEAR_SPAN || span / gaps >= YEAR_GAP {
        XUnit::Year
    } else {
        XUnit::Month
    }
}

/// An axis mark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Tick {
    /// "2.5M", "500K", "250".
    pub label: String,
    pub pos: i64,
}

/// A graph over dates (or named categories): bars and lines against one
/// money axis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Chart {
    /// One per value; empty when the graph is by `labels`.
    pub dates: Vec<Date>,
    /// Category names instead of dates (an asset class per bar); empty
    /// for a graph over dates.
    pub labels: Vec<String>,
    pub series: Vec<Series>,
    pub ticks: Vec<Tick>,
    /// Where zero sits: bars grow from here.
    pub zero: i64,
    /// What the date axis names (a graph over dates).
    pub x_unit: XUnit,
}

/// A round step near a fifth of `span` cents: 1, 2, 2.5, or 5 times a
/// power of ten, at least `min_step` cents.
fn step_for(span: i128, min_step: i128) -> i128 {
    let raw = (span / 5).max(min_step);
    let mut p: i128 = 1;
    while p * 10 <= raw {
        p *= 10;
    }
    let mut candidates = vec![p, 2 * p];
    if p >= 10 {
        candidates.push(p * 5 / 2);
    }
    candidates.extend([5 * p, 10 * p]);
    candidates.sort_unstable();
    candidates.into_iter().find(|c| *c >= raw).unwrap_or(10 * p)
}

/// A tick label in dollars: millions as "M", thousands as "K".
fn tick_label(cents: i128, step: i128) -> String {
    if cents == 0 {
        return "0".into();
    }
    let dollars = Decimal::from_i128_with_scale(cents, 2);
    let (value, suffix) = if step >= 100_000_000 {
        (dollars / Decimal::from(1_000_000), "M")
    } else if step >= 100_000 {
        (dollars / Decimal::from(1_000), "K")
    } else {
        (dollars, "")
    };
    format!("{}{suffix}", value.round_dp(2).normalize())
}

fn to_i64(v: i128) -> Result<i64> {
    i64::try_from(v).map_err(|_| Error::Overflow("chart"))
}

/// A graph over named categories. Every series has one value per label.
pub fn build_labeled(
    labels: Vec<String>,
    series: Vec<(String, SeriesStyle, Vec<Money>)>,
) -> Result<Chart> {
    let mut c = build(Vec::new(), series)?;
    c.labels = labels;
    Ok(c)
}

/// Build a graph. Every series has one value per date. The axis always
/// includes zero and steps by at least a dollar.
pub fn build(dates: Vec<Date>, series: Vec<(String, SeriesStyle, Vec<Money>)>) -> Result<Chart> {
    build_range(dates, series, false)
}

/// Build a graph whose axis fits the data's own range instead of reaching
/// zero, with steps down to a cent, so a price that moves a few cents
/// shows the movement. The line never touches the top or bottom.
pub fn build_fitted(
    dates: Vec<Date>,
    series: Vec<(String, SeriesStyle, Vec<Money>)>,
) -> Result<Chart> {
    build_range(dates, series, true)
}

fn build_range(
    dates: Vec<Date>,
    series: Vec<(String, SeriesStyle, Vec<Money>)>,
    fitted: bool,
) -> Result<Chart> {
    let all = series
        .iter()
        .flat_map(|(_, _, v)| v.iter().map(|m| i128::from(m.cents())));
    let (mut lo, mut hi) = (all.clone().min().unwrap_or(0), all.max().unwrap_or(0));
    if !fitted {
        lo = lo.min(0);
        hi = hi.max(0);
    } else if lo == hi {
        // A flat line: room above and below.
        let pad = (lo.abs() / 20).max(1);
        lo -= pad;
        hi += pad;
    }
    let step = step_for((hi - lo).max(1), if fitted { 1 } else { 100 });
    let mut lo_t = lo.div_euclid(step) * step;
    let mut hi_t = -((-hi).div_euclid(step)) * step;
    if fitted {
        if lo_t == lo {
            lo_t -= step;
        }
        if hi_t == hi {
            hi_t += step;
        }
    }
    if hi_t <= lo_t {
        hi_t = lo_t + step;
    }
    let span = to_i64(hi_t - lo_t)?;
    let pos = |cents: i128| -> Result<i64> { mul_div(to_i64(cents - lo_t)?, SCALE, span) };
    let mut ticks = Vec::new();
    let mut t = lo_t;
    while t <= hi_t {
        ticks.push(Tick {
            label: tick_label(t, step),
            pos: pos(t)?,
        });
        t += step;
    }
    let series = series
        .into_iter()
        .map(|(name, style, values)| {
            let p = values
                .iter()
                .map(|m| pos(i128::from(m.cents())))
                .collect::<Result<Vec<_>>>()?;
            Ok(Series {
                name,
                style,
                values,
                pos: p,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Chart {
        x_unit: x_unit(&dates),
        dates,
        labels: Vec::new(),
        series,
        ticks,
        // Off the plot when a fitted range leaves zero out: keep it on the edge.
        zero: pos(0)?.clamp(0, SCALE),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(s: &str) -> Money {
        s.parse().unwrap()
    }

    #[test]
    fn axis_is_round_and_includes_zero() {
        let d: Date = "2026-01-31".parse().unwrap();
        let c = build(
            vec![d, d],
            vec![(
                "Net".into(),
                SeriesStyle::Line,
                vec![m("9400000.00"), m("10100000.00")],
            )],
        )
        .unwrap();
        let labels: Vec<_> = c.ticks.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, ["0", "2.5M", "5M", "7.5M", "10M", "12.5M"]);
        assert_eq!(c.zero, 0);
        assert_eq!(c.ticks.last().unwrap().pos, SCALE);
        // 9.4M of 12.5M.
        assert_eq!(c.series[0].pos[0], 7520);
    }

    #[test]
    fn negative_values_put_zero_above_the_bottom() {
        let d: Date = "2026-01-31".parse().unwrap();
        let c = build(
            vec![d],
            vec![
                ("x".into(), SeriesStyle::Bar, vec![m("-250.00")]),
                ("y".into(), SeriesStyle::Bar, vec![m("750.00")]),
            ],
        )
        .unwrap();
        // Axis -400 .. 800 in steps of 200.
        let labels: Vec<_> = c.ticks.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, ["-400", "-200", "0", "200", "400", "600", "800"]);
        assert_eq!(c.zero, 3333);
        assert_eq!(c.series[0].pos[0], 1250);
        assert_eq!(c.series[1].pos[0], 9583);
    }

    fn prices(list: &[&str]) -> Chart {
        let d: Date = "2026-01-31".parse().unwrap();
        build_fitted(
            vec![d; list.len()],
            vec![(
                "Price".into(),
                SeriesStyle::Line,
                list.iter().map(|s| m(s)).collect(),
            )],
        )
        .unwrap()
    }

    #[test]
    fn a_fitted_axis_follows_the_data_in_sub_dollar_steps() {
        let c = prices(&["10.10", "10.50", "10.30"]);
        let labels: Vec<_> = c.ticks.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(
            labels,
            ["10", "10.1", "10.2", "10.3", "10.4", "10.5", "10.6"]
        );
        assert_eq!(c.ticks[0].pos, 0);
        assert_eq!(c.ticks.last().unwrap().pos, SCALE);
        // The line stays off the top and bottom.
        let pos = &c.series[0].pos;
        assert!(pos.iter().all(|p| *p > 0 && *p < SCALE));
        // Zero is off the plot: kept on the bottom edge.
        assert_eq!(c.zero, 0);
    }

    #[test]
    fn a_fitted_axis_can_name_quarter_steps() {
        let c = prices(&["10.00", "11.25"]);
        let labels: Vec<_> = c.ticks.iter().map(|t| t.label.as_str()).collect();
        assert!(labels.contains(&"10.25") || labels.contains(&"10.5"));
        assert!(c.series[0].pos[0] > 0 && c.series[0].pos[1] < SCALE);
    }

    #[test]
    fn a_fitted_flat_series_gets_room_each_side() {
        let c = prices(&["10.00", "10.00"]);
        assert!(c.ticks.len() >= 3);
        let p = c.series[0].pos[0];
        assert!(p > 0 && p < SCALE);
        assert_eq!(p, c.series[0].pos[1]);
        let zero = prices(&["0.00"]);
        assert!(zero.ticks.len() >= 2);
    }

    #[test]
    fn a_fitted_axis_through_zero_keeps_zero_in_place() {
        let c = prices(&["-2.00", "3.00"]);
        assert!(c.zero > 0 && c.zero < SCALE);
    }

    #[test]
    fn empty_chart_still_has_an_axis() {
        let c = build(Vec::new(), Vec::new()).unwrap();
        assert_eq!(c.ticks.len(), 2);
    }

    /// A graph's date axis (RPT-010).
    #[test]
    fn the_date_axis_names_days_months_or_years() {
        let d = |s: &str| -> Date { s.parse().unwrap() };
        let unit = |ds: &[&str]| x_unit(&ds.iter().map(|s| d(s)).collect::<Vec<_>>());
        assert_eq!(unit(&["2026-09-23", "2026-09-30"]), XUnit::Day);
        assert_eq!(unit(&["2026-07-01", "2026-09-30"]), XUnit::Day);
        assert_eq!(
            unit(&["2025-09-30", "2026-03-31", "2026-09-30"]),
            XUnit::Month
        );
        assert_eq!(unit(&["2021-09-30", "2026-09-30"]), XUnit::Year);
        // Year-end points two years apart.
        assert_eq!(unit(&["2024-12-31", "2025-12-31"]), XUnit::Year);
        assert_eq!(unit(&[]), XUnit::Month);
    }
}
