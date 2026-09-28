import { describe, expect, it } from "vitest";
import { columnHeading, formatCell } from "../format/report";
import type { Column, Row } from "../types/bindings";
import { MENU_REPORTS, REPORTS, toggleFilter } from "./meta";
import { flatten } from "./rows";

const row = (label: string, cells: string[], children: Row[] = [], kind: Row["kind"] = "group"): Row => ({
  kind: children.length ? kind : "detail",
  label,
  cells,
  drill: null,
  children,
});

describe("flatten", () => {
  const tree: Row[] = [
    row("EXPENSES", ["", "-15.00"], [
      row("Food", ["", "-15.00"], [row("", ["2026-01-01", "-10.00"]), row("", ["2026-01-02", "-5.00"])]),
    ], "section"),
    { kind: "total", label: "OVERALL TOTAL", cells: ["", "-15.00"], drill: null, children: [] },
  ];

  it("expanded groups get a heading without figures and a closing total", () => {
    const lines = flatten(tree, () => false);
    expect(lines.map((l) => [l.depth, l.label, l.cells[1]])).toEqual([
      [0, "EXPENSES", ""],
      [1, "Food", ""],
      [2, "", "-10.00"],
      [2, "", "-5.00"],
      [1, "Total Food", "-15.00"],
      [0, "Total EXPENSES", "-15.00"],
      [0, "OVERALL TOTAL", "-15.00"],
    ]);
    expect(lines[0].path).toBe("0");
    expect(lines[1].path).toBe("0/0");
    expect(lines[4].closing).toBe(true);
  });

  it("a collapsed group is one line with its figures", () => {
    const lines = flatten(tree, (p) => p === "0/0");
    expect(lines.map((l) => [l.label, l.cells[1], l.collapsed])).toEqual([
      ["EXPENSES", "", false],
      ["Food", "-15.00", true],
      ["Total EXPENSES", "-15.00", false],
      ["OVERALL TOTAL", "-15.00", false],
    ]);
  });
});

describe("toggleFilter", () => {
  const all = [1, 2, 3];
  it("all chosen is null, so later additions are included", () => {
    expect(toggleFilter(null, all, all, 2, false)).toEqual([1, 3]);
    expect(toggleFilter([1, 3], all, all, 2, true)).toBeNull();
  });
  it("a narrower default never collapses to null", () => {
    expect(toggleFilter(null, [1], all, 2, true)).toEqual([1, 2]);
    expect(toggleFilter([1, 2], [1], all, 3, true)).toEqual([1, 2, 3]);
  });
});

describe("report cells", () => {
  const col = (kind: Column["kind"], label = "X", from: string | null = null, to: string | null = null): Column => ({ id: "x", label, kind, from, to });
  it("formats by column kind", () => {
    expect(formatCell("money", "-1234.50", true)).toBe("-1,234.50");
    expect(formatCell("money", "1235.00", false)).toBe("1,235");
    expect(formatCell("quantity", "1234.5", true)).toBe("1,234.5");
    expect(formatCell("date", "2026-03-05", true)).toBe("03/05/2026");
    expect(formatCell("text", "", true)).toBe("");
  });
  it("balance and period headings show dates", () => {
    expect(columnHeading(col("money", "Balance", null, "2026-01-31"))).toEqual(["01/31/2026", "Balance"]);
    expect(columnHeading(col("money", "", "2026-01-01", "2026-01-07"))).toEqual(["01/01/2026", "– 01/07/2026"]);
    expect(columnHeading(col("money", "Q1 2026", "2026-01-01", "2026-03-31"))).toEqual(["Q1 2026"]);
  });
});

describe("report menu", () => {
  it("every menu report is known and Capital Gains is listed twice", () => {
    for (const kind of Object.values(MENU_REPORTS)) expect(REPORTS[kind]).toBeDefined();
    expect(Object.values(MENU_REPORTS).filter((k) => k === "capital_gains")).toHaveLength(2);
  });
});
