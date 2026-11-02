// A graph's date axis labels (RPT-010). Rust says what the axis names
// (days, months, or years); this picks which points get a label: the
// first point of each day, month, or year, thinned so labels stand at
// least a fair share of the axis apart (at most `max` of them). Days and
// months name the year on a second line, under the first label and
// wherever the year changes, so each label stays short.
// Display only: dates are compared as ISO text, never as JS dates.

import { dayShort, monthShort } from "../format/date";
import type { XUnit } from "../types/bindings";

export interface AxisLabel {
  /** The point's index. */
  i: number;
  text: string;
  /** The year, on a second line; null where it has not changed. */
  sub: string | null;
}

const keyOf = (iso: string, unit: XUnit) =>
  unit === "day" ? iso : unit === "month" ? iso.slice(0, 7) : iso.slice(0, 4);

const textOf = (iso: string, unit: XUnit) =>
  unit === "day" ? dayShort(iso) : unit === "month" ? monthShort(iso).slice(0, 3) : iso.slice(0, 4);

export function dateLabels(dates: string[], unit: XUnit, max = 12): AxisLabel[] {
  const firsts: number[] = [];
  dates.forEach((d, i) => {
    if (i === 0 || keyOf(d, unit) !== keyOf(dates[i - 1], unit)) firsts.push(i);
  });
  // Points need not be evenly spread (a month with few prices), so keep
  // a label only when it is far enough past the last one kept.
  const gap = firsts.length > max ? dates.length / max : 0;
  const kept: number[] = [];
  for (const i of firsts) {
    if (kept.length === 0 || i - kept[kept.length - 1] >= gap) kept.push(i);
  }
  let year = "";
  return kept.map((i) => {
    const y = dates[i].slice(0, 4);
    const sub = unit !== "year" && y !== year ? y : null;
    year = y;
    return { i, text: textOf(dates[i], unit), sub };
  });
}
