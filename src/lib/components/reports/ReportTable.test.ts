import { describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { SvelteSet } from "svelte/reactivity";
import { beforeEach } from "vitest";
import ReportTable from "./ReportTable.svelte";
import type { Report } from "../../types/bindings";

// A long report draws only the lines in view; Save PDF draws them all;
// a compact report does not measure again on collapse or expand.

const detail = (i: number, cells: string[]) => ({
  kind: "detail" as const,
  label: "",
  cells,
  drill: { kind: "txn" as const, account: 1, txn: i, date: "2025-04-01" },
  children: [],
});

const big = (n: number, compact: boolean): Report =>
  ({
    kind: compact ? "tax_schedule" : "itemized_categories",
    title: "Big",
    note: "",
    from: "2025-01-01",
    to: "2025-12-31",
    as_of: false,
    cents: true,
    columns: [
      { id: "date", label: "Date", kind: "date", from: null, to: null },
      { id: "description", label: "Description", kind: "text", from: null, to: null },
      { id: "amount", label: "Amount", kind: "money", from: null, to: null },
    ],
    rows: [
      {
        kind: "section",
        label: "Food",
        cells: ["", "", "-100.00"],
        drill: null,
        children: Array.from({ length: n }, (_, i) => detail(i, ["2025-04-01", `Payee ${i}`, "-1.00"])),
      },
    ],
    chart: null,
    totals_on_heading: compact,
    compact,
  }) as unknown as Report;

function collapsible() {
  const closed = new SvelteSet<string>();
  return { isCollapsed: (p: string) => closed.has(p), toggle: (p: string) => (closed.has(p) ? closed.delete(p) : closed.add(p)) };
}

beforeEach(() => cleanup());

describe("ReportTable", () => {
  it("draws only a window of a long report, and every line for Save PDF", async () => {
    const r = render(ReportTable, { report: big(1500, false), inst: collapsible(), ondrill: vi.fn() });
    const drawn = () => document.querySelectorAll("tbody tr[data-i]").length;
    await waitFor(() => expect(drawn()).toBeGreaterThan(0));
    expect(drawn()).toBeLessThan(300);
    // The rest is stood for by a spacer row.
    expect(document.querySelectorAll("tbody tr.gap")).toHaveLength(1);
    await r.component.preparePrint();
    expect(drawn()).toBe(1502);
    expect(document.querySelectorAll("tbody tr.gap")).toHaveLength(0);
    r.component.endPrint();
    await waitFor(() => expect(drawn()).toBeLessThan(300));
  });

  it("draws a short report whole", async () => {
    render(ReportTable, { report: big(10, false), inst: collapsible(), ondrill: vi.fn() });
    await waitFor(() => expect(document.querySelectorAll("tbody tr[data-i]")).toHaveLength(12));
    expect(document.querySelector("tbody tr.gap")).toBeNull();
  });

  it("a compact report keeps its widths when a group collapses or expands", async () => {
    render(ReportTable, { report: big(20, true), inst: collapsible(), ondrill: vi.fn() });
    const table = () => document.querySelector("table.report.compact")!;
    await waitFor(() => expect(table().classList.contains("measuring")).toBe(false));
    await fireEvent.click(document.querySelector("button.tog")!);
    expect(table().classList.contains("measuring")).toBe(false);
    expect(document.querySelectorAll("tbody tr")).toHaveLength(1);
    await fireEvent.click(document.querySelector("button.tog")!);
    expect(table().classList.contains("measuring")).toBe(false);
    expect(document.querySelectorAll("tbody tr")).toHaveLength(21);
  });

  it("a long compact report measures only its widest lines, then windows", async () => {
    render(ReportTable, { report: big(1500, true), inst: collapsible(), ondrill: vi.fn() });
    const table = () => document.querySelector("table.report.compact")!;
    await waitFor(() => expect(table().classList.contains("measuring")).toBe(false));
    expect(document.querySelectorAll("tbody tr[data-i]").length).toBeLessThan(300);
  });
});
