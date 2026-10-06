import { describe, expect, it } from "vitest";
import { columnHeading, formatCell } from "../format/report";
import type { Column, Row } from "../types/bindings";
import { MENU_REPORTS, REPORTS, compareChoices, compareFor, compareGroups, presetGroups, toggleFilter } from "./meta";
import { fitColumns, MIN_CUT, paginate } from "./fit";
import { closingLabel, flatten } from "./rows";

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

describe("flatten, totals on the heading", () => {
  const tree: Row[] = [
    row("EXPENSES", ["", "-15.00"], [
      row("Food", ["", "-15.00"], [row("", ["2026-01-01", "-10.00"]), row("", ["2026-01-02", "-5.00"])]),
    ], "section"),
    { kind: "total", label: "OVERALL TOTAL", cells: ["", "-15.00"], drill: null, children: [] },
  ];

  it("headings carry the figures and no closing lines follow", () => {
    const lines = flatten(tree, () => false, true);
    expect(lines.map((l) => [l.depth, l.label, l.cells[1]])).toEqual([
      [0, "EXPENSES", "-15.00"],
      [1, "Food", "-15.00"],
      [2, "", "-10.00"],
      [2, "", "-5.00"],
      [0, "OVERALL TOTAL", "-15.00"],
    ]);
    expect(lines.some((l) => l.closing)).toBe(false);
  });

  it("a collapsed group is still one line", () => {
    const lines = flatten(tree, (p) => p === "0/0", true);
    expect(lines.map((l) => [l.label, l.cells[1], l.collapsed])).toEqual([
      ["EXPENSES", "-15.00", false],
      ["Food", "-15.00", true],
      ["OVERALL TOTAL", "-15.00", false],
    ]);
  });

  it("a label that says Total gets no second one", () => {
    expect(closingLabel("Total IRA taxable distrib.")).toBe("Total IRA taxable distrib.");
    expect(closingLabel("Food")).toBe("Total Food");
  });
});

describe("fitColumns", () => {
  const ids = ["date", "account", "description", "memo", "category", "tag", "amount"];
  const natural = [6, 12, 20, 14, 15, 8, 7]; // 82
  const total = (w: number[]) => w.reduce((a, b) => a + b, 0);

  it("leaves columns that fit alone", () => {
    expect(fitColumns(ids, natural, 90)).toEqual({ widths: natural, scale: 1 });
  });

  it("cuts Description, Memo, and Tag first, the widest first", () => {
    const f = fitColumns(ids, natural, 76);
    expect(total(f.widths)).toBeCloseTo(76, 6);
    expect(f.widths[2]).toBeCloseTo(14, 6);
    expect(f.widths[3]).toBeCloseTo(14, 6);
    expect(f.widths[5]).toBe(8);
    expect([f.widths[0], f.widths[1], f.widths[4], f.widths[6]]).toEqual([6, 12, 15, 7]);
    expect(f.scale).toBe(1);
  });

  it("then Account; Category is never cut", () => {
    // Description, Memo, Tag at the floor: 6 + 12 + 15 + 7 + 3 * MIN_CUT = 55.
    const f = fitColumns(ids, natural, 50);
    expect([f.widths[2], f.widths[3], f.widths[5]]).toEqual([MIN_CUT, MIN_CUT, MIN_CUT]);
    expect(f.widths[1]).toBeCloseTo(7, 6);
    expect(f.widths[4]).toBe(15);
    expect(f.scale).toBe(1);
  });

  it("Tax Summary: Tax Item is cut with Account, the widest first", () => {
    const f = fitColumns(["description", "account", "tax_item", "category"], [5, 10, 20, 15], 40);
    expect(f.widths[2]).toBeCloseTo(10, 6);
    expect(f.widths[1]).toBe(10);
    expect(f.widths[3]).toBe(15);
    expect(f.scale).toBe(1);
  });

  it("scales what still does not fit", () => {
    const f = fitColumns(ids, natural, 40);
    expect(f.widths[1]).toBe(MIN_CUT);
    const w = total(f.widths);
    expect(w).toBe(6 + 15 + 7 + 4 * MIN_CUT);
    expect(f.scale).toBeCloseTo(40 / w, 9);
  });

  it("a narrow column is never widened to the floor", () => {
    const f = fitColumns(["tag", "category"], [3, 30], 20);
    expect(f.widths).toEqual([3, 30]);
    expect(f.scale).toBeCloseTo(20 / 33, 9);
  });
});

describe("paginate", () => {
  const d = { h: 10, heading: false };
  const hd = { h: 10, heading: true };

  it("fills each page under the column headings; the first page under the title", () => {
    // Page 100, headings 10, title 30: 6 rows on page 1, then 9 a page.
    expect(paginate(Array(20).fill(d), 10, 100, 30)).toEqual([0, 6, 15]);
  });

  it("a heading never ends a page", () => {
    const rows = [d, d, d, d, hd, hd, d, d, d];
    // Rows 4 and 5 (a form and its line) would end page 1: they move.
    expect(paginate(rows, 10, 100, 30)).toEqual([0, 4]);
  });

  it("one page when everything fits", () => {
    expect(paginate([hd, d, d], 10, 100, 30)).toEqual([0]);
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
    expect(formatCell("percent", "-12.34", false)).toBe("-12.34%");
    expect(formatCell("percent", "", true)).toBe("");
  });
  it("balance and period headings show dates", () => {
    expect(columnHeading(col("money", "Balance", null, "2026-01-31"))).toEqual(["01/31/2026", "Balance"]);
    expect(columnHeading(col("money", "", "2026-01-01", "2026-01-07"))).toEqual(["01/01/2026", "– 01/07/2026"]);
    expect(columnHeading(col("money", "Q1 2026", "2026-01-01", "2026-03-31"))).toEqual(["Q1 2026"]);
    // Comparison reports (RPT-210).
    const cmp = (id: string, label: string, from: string | null, to: string | null) => ({ ...col("money", label, from, to), id });
    expect(columnHeading(cmp("current", "Last quarter", "2026-07-01", "2026-09-30"))).toEqual(["Last quarter"]);
    const avg = (label: string) => columnHeading(cmp("average", label, "2025-10-01", "2026-09-30"));
    expect(avg("Last 4 quarters")).toEqual(["Avg spending", "Last 4 qtrs"]);
    expect(avg("Last 12 weeks")).toEqual(["Avg spending", "Last 12 wks"]);
    expect(avg("Last 6 months")).toEqual(["Avg spending", "Last 6 months"]);
    expect(avg("Last 3 years")).toEqual(["Avg spending", "Last 3 yrs"]);
    expect(avg("Last year")).toEqual(["Avg spending", "Last year"]);
  });
});

describe("report menu", () => {
  it("every menu report is known and Capital Gains is listed twice", () => {
    for (const kind of Object.values(MENU_REPORTS)) expect(REPORTS[kind]).toBeDefined();
    expect(Object.values(MENU_REPORTS).filter((k) => k === "capital_gains")).toHaveLength(2);
  });
});

describe("presetGroups (RPT-040)", () => {
  const labels = (g: ReturnType<typeof presetGroups>) => g.map((x) => x.map(([, l]) => l));
  const rest = [
    ["Month to date", "Quarter to date", "Year to date"],
    ["Last month", "Last quarter", "Last year", "Last 30 days", "Last 12 months", "Custom dates"],
  ];

  it("Tax Schedule: all dates, then Monthly/Quarterly/Yearly, then the rest", () => {
    expect(labels(presetGroups("tax_schedule", "last_year"))).toEqual([
      ["Include all dates"],
      ["Monthly", "Quarterly", "Yearly"],
      ...rest,
    ]);
  });

  it("other reports: no period group, and no This month/quarter/year", () => {
    const g = presetGroups("itemized_categories", "year_to_date");
    expect(labels(g)).toEqual([["Include all dates"], ...rest]);
    expect(g.flat().map(([p]) => p)).not.toContain("this_month");
  });

  it("a saved report's old preset is added at the end", () => {
    expect(labels(presetGroups("net_worth", "this_month")).at(-1)).toEqual(["This month"]);
  });
});

describe("comparison reports (RPT-210)", () => {
  const values = (l: [string, string][]) => l.map(([v]) => v);

  it("offer current and last week, month, quarter, year, and custom", () => {
    expect(presetGroups("compare_category", "month_to_date").map((g) => g.map(([, l]) => l))).toEqual([
      ["Current week", "Current month", "Current quarter", "Current year"],
      ["Last week", "Last month", "Last quarter", "Last year", "Custom dates"],
    ]);
  });

  it("Compare to follows the date range; custom offers every choice", () => {
    expect(values(compareChoices("week_to_date"))).toEqual(["weeks_4", "weeks_8", "weeks_12"]);
    expect(values(compareChoices("last_month"))).toEqual(["months_3", "months_6", "months_12"]);
    expect(values(compareChoices("quarter_to_date"))).toEqual(["quarters_4", "quarters_8", "quarters_12"]);
    expect(compareChoices("last_year").map(([, l]) => l)).toEqual(["Last year", "Last 3 years", "Last 5 years"]);
    expect(compareChoices("custom")).toHaveLength(12);
    expect(compareFor("last_month", "weeks_8")).toBe("months_3");
    expect(compareFor("custom", "weeks_8")).toBe("weeks_8");
    expect(compareFor("year_to_date", undefined)).toBe("years_1");
  });

  it("Subtotal by: the category report offers Payee, the payee report Category", () => {
    expect(values(compareGroups("compare_category"))).toEqual(["none", "payee", "tag", "account", "tax_line"]);
    expect(values(compareGroups("compare_payee"))).toEqual(["none", "category", "tag", "account", "tax_line"]);
  });
});
