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

/// An axis mark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Tick {
    /// "2.5M", "500K", "250".
    pub label: String,
    pub pos: i64,
}

/// A graph over dates: bars and lines against one money axis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Chart {
    pub dates: Vec<Date>,
    pub series: Vec<Series>,
    pub ticks: Vec<Tick>,
    /// Where zero sits: bars grow from here.
    pub zero: i64,
}

/// A round step near a fifth of `span` cents: 1, 2, 2.5, or 5 times a
/// power of ten, at least one dollar.
fn step_for(span: i128) -> i128 {
    let raw = (span / 5).max(100);
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

/// Build a graph. Every series has one value per date.
pub fn build(dates: Vec<Date>, series: Vec<(String, SeriesStyle, Vec<Money>)>) -> Result<Chart> {
    let all = series
        .iter()
        .flat_map(|(_, _, v)| v.iter().map(|m| i128::from(m.cents())));
    let lo = all.clone().min().unwrap_or(0).min(0);
    let hi = all.max().unwrap_or(0).max(0);
    let step = step_for((hi - lo).max(1));
    let lo_t = lo.div_euclid(step) * step;
    let mut hi_t = -((-hi).div_euclid(step)) * step;
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
        dates,
        series,
        ticks,
        zero: pos(0)?,
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

    #[test]
    fn empty_chart_still_has_an_axis() {
        let c = build(Vec::new(), Vec::new()).unwrap();
        assert_eq!(c.ticks.len(), 2);
    }
}
