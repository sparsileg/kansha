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
      { i: 0, text: "Jul 2026" },
      { i: 2, text: "Aug 2026" },
      { i: 4, text: "Sep 2026" },
    ]);
  });

  it("names years, and thins to the most labels that fit", () => {
    const years = ["2021", "2022", "2023", "2024", "2025", "2026"].map((y) => `${y}-06-30`);
    expect(dateLabels(years, "year").map((l) => l.text)).toEqual(["2021", "2022", "2023", "2024", "2025", "2026"]);
    expect(dateLabels(years, "year", 3).map((l) => l.text)).toEqual(["2021", "2023", "2025"]);
  });
});
