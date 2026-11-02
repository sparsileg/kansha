import { beforeEach, describe, expect, it } from "vitest";
import { dateLabels } from "./axis";
import { dateFormatState } from "../state/dateformat.svelte";

beforeEach(() => dateFormatState.set("mdy"));

describe("date axis labels", () => {
  it("names each day for a short span", () => {
    const days = ["2026-09-23", "2026-09-24", "2026-09-25"];
    expect(dateLabels(days, "day").map((l) => l.text)).toEqual(["9/23", "9/24", "9/25"]);
    dateFormatState.set("dmy");
    expect(dateLabels(days, "day")[0].text).toBe("23/9");
  });

  it("names a month once, at its first point, however many points it has", () => {
    const dates = ["2026-07-30", "2026-07-31", "2026-08-03", "2026-08-04", "2026-09-01"];
    expect(dateLabels(dates, "month")).toEqual([
      { i: 0, text: "Jul", sub: "2026" },
      { i: 2, text: "Aug", sub: null },
      { i: 4, text: "Sep", sub: null },
    ]);
  });

  it("names years, and thins to the most labels that fit", () => {
    const years = ["2021", "2022", "2023", "2024", "2025", "2026"].map((y) => `${y}-06-30`);
    expect(dateLabels(years, "year").map((l) => l.text)).toEqual(["2021", "2022", "2023", "2024", "2025", "2026"]);
    expect(dateLabels(years, "year", 3).map((l) => l.text)).toEqual(["2021", "2023", "2025"]);
    expect(dateLabels(years, "year").every((l) => l.sub === null)).toBe(true);
  });

  it("names the year under the first month and where it changes", () => {
    const dates = ["2025-11-28", "2025-12-31", "2026-01-30", "2026-02-27"];
    expect(dateLabels(dates, "month").map((l) => [l.text, l.sub])).toEqual([
      ["Nov", "2025"],
      ["Dec", null],
      ["Jan", "2026"],
      ["Feb", null],
    ]);
  });

  it("thins by distance along the axis, so crowded months do not overlap", () => {
    // One price a month to June, then 20 a month: a label every k-th
    // month would crowd the sparse half; by distance it cannot.
    const dates: string[] = [];
    for (let m = 1; m <= 12; m++) {
      const mm = String(m).padStart(2, "0");
      for (let d = 1; d <= (m <= 6 ? 1 : 20); d++) dates.push(`2026-${mm}-${String(d).padStart(2, "0")}`);
    }
    const labels = dateLabels(dates, "month", 6);
    const gap = dates.length / 6;
    labels.slice(1).forEach((l, k) => expect(l.i - labels[k].i).toBeGreaterThanOrEqual(gap));
    // A year thinned out of the labels still shows on the next one.
    const two = ["2025-10-31", "2025-11-28", "2025-12-31", "2026-01-30", "2026-02-27", "2026-03-31"];
    expect(dateLabels(two, "month", 2).map((l) => [l.text, l.sub])).toEqual([
      ["Oct", "2025"],
      ["Jan", "2026"],
    ]);
  });
});
