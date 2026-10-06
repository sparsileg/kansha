import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import SpendingTable from "./SpendingTable.svelte";
import type { ExpenseCard } from "../../types/bindings";

const ROW = 20;
const card = (n: number): ExpenseCard => {
  const row = (i: number) => ({
    category: i,
    label: `Cat ${i}`,
    ytd: "1.00",
    mtd: "0.00",
    monthly_avg: "0.10",
    scheduled: i === 2 ? "0.50" : "0.00",
  });
  return {
    rows: Array.from({ length: n }, (_, i) => row(i + 1)),
    total: {
      category: null,
      label: "Total",
      ytd: `${n}.00`,
      mtd: "0.00",
      monthly_avg: "0.00",
      scheduled: "0.50",
    },
    chosen: n,
  };
};

// jsdom has no layout: every row is 20 px; the scroller shows what its
// max height allows.
beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    height: ROW,
  } as DOMRect);
  Object.defineProperty(HTMLElement.prototype, "scrollHeight", {
    configurable: true,
    get(this: HTMLElement) {
      return this.querySelectorAll("tr").length * ROW;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get(this: HTMLElement) {
      const max = parseFloat(this.style.maxHeight);
      return Number.isNaN(max) ? this.scrollHeight : max;
    },
  });
  HTMLElement.prototype.scrollBy = vi.fn();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const frame = () => new Promise((r) => requestAnimationFrame(() => r(null)));

describe("spending card rows (CARD-060)", () => {
  it("shows every row, with no count or arrows, up to the setting", async () => {
    render(SpendingTable, { card: card(10), rows: 10 });
    await frame();
    expect(screen.getAllByRole("row")).toHaveLength(12);
    expect(screen.queryByRole("region")).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("past the setting, scrolls between header and Total, with a count and two pulsing arrows toward more", async () => {
    const { container } = render(SpendingTable, { card: card(24), rows: 10 });
    await frame();
    const region = screen.getByRole("region", { name: "Categories" });
    expect(region.style.maxHeight).toBe(`${12 * ROW}px`);
    expect(region.getAttribute("tabindex")).toBe("0");
    expect(
      screen.getByRole("button", { name: "Rows 1–10 of 24" }),
    ).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Scroll up" })).toBeNull();
    const downs = screen.getAllByRole("button", { name: "Scroll down" });
    expect(downs).toHaveLength(2);
    await fireEvent.click(downs[1]);
    expect(region.scrollBy).toHaveBeenCalledWith({
      top: 10 * ROW,
      behavior: "smooth",
    });

    // At the end: the arrow points back up; the count follows.
    region.scrollTop = 14 * ROW;
    await fireEvent.scroll(region);
    expect(
      screen.getByRole("button", { name: "Rows 15–24 of 24" }),
    ).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Scroll down" })).toBeNull();
    expect(screen.getAllByRole("button", { name: "Scroll up" })).toHaveLength(
      2,
    );
    expect(container.querySelector("tfoot")?.textContent).toMatch(/Total/);
  });

  it("clicking the count shows every row, and again scrolls", async () => {
    render(SpendingTable, { card: card(24), rows: 10 });
    await frame();
    await fireEvent.click(
      screen.getByRole("button", { name: "Rows 1–10 of 24" }),
    );
    expect(screen.queryByRole("region")).toBeNull();
    expect(screen.queryByRole("button", { name: /Scroll/ })).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "All 24 rows" }));
    expect(screen.getByRole("region", { name: "Categories" })).toBeTruthy();
  });

  it("heads the columns Year and Month; a figure with scheduled spending says how much", async () => {
    const { container } = render(SpendingTable, { card: card(3), rows: 10 });
    await frame();
    const head = [...container.querySelectorAll("thead th")].map(
      (c) => c.textContent,
    );
    expect(head).toEqual(["Category", "Year", "Month", "Monthly Avg"]);
    const cells = (i: number) => [
      ...container.querySelectorAll("tbody tr")[i].querySelectorAll("td"),
    ];
    expect(cells(1).map((c) => c.getAttribute("title"))).toEqual([
      "Cat 2",
      "0.50 scheduled",
      "0.50 scheduled",
      null,
    ]);
    expect(cells(0)[1].getAttribute("title")).toBeNull();
    expect(container.querySelector("tfoot td.num")?.getAttribute("title")).toBe(
      "0.50 scheduled",
    );
  });
});
