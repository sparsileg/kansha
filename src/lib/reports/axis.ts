// A graph's date axis labels (RPT-010). Rust says what the axis names
// (days, months, or years); this picks which points get a label: the
// first point of each day, month, or year, thinned to at most `max`.
// Display only: dates are compared as ISO text, never as JS dates.

import { dayShort, monthShort } from "../format/date";
import type { XUnit } from "../types/bindings";

export interface AxisLabel {
  /** The point's index. */
  i: number;
  text: string;
}

const keyOf = (iso: string, unit: XUnit) =>
  unit === "day" ? iso : unit === "month" ? iso.slice(0, 7) : iso.slice(0, 4);

const textOf = (iso: string, unit: XUnit) =>
  unit === "day" ? dayShort(iso) : unit === "month" ? monthShort(iso) : iso.slice(0, 4);

export function dateLabels(dates: string[], unit: XUnit, max = 12): AxisLabel[] {
  const firsts: number[] = [];
  dates.forEach((d, i) => {
    if (i === 0 || keyOf(d, unit) !== keyOf(dates[i - 1], unit)) firsts.push(i);
  });
  const every = Math.max(1, Math.ceil(firsts.length / max));
  return firsts.filter((_, k) => k % every === 0).map((i) => ({ i, text: textOf(dates[i], unit) }));
}
