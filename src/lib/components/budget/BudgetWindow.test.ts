import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

const fig = (budget: string, actual: string, balance: string) => ({ budget, actual, balance });
const zero = fig("0.00", "0.00", "0.00");
const SHORT = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const LONG = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

function row(category: number | null, prefix: string, name: string, oct = zero, children: unknown[] = []) {
  const months = Array.from({ length: 12 }, (_, i) => (i === 9 ? oct : zero));
  return {
    category,
    prefix,
    name,
    months,
    to_date: oct,
    year: oct,
    entered: Array.from({ length: 12 }, (_, i) => (i === 9 ? "102.50" : "0.00")),
    counted: category === null ? [9] : [category],
    children,
  };
}

const view = {
  budget: { id: 1, name: "Household", categories: [3], years: [2026] },
  year: 2026,
  held: true,
  current: 10,
  to_date: 10,
  columns: SHORT.map((short, i) => ({
    month: i + 1,
    short,
    long: LONG[i],
    from: `2026-${String(i + 1).padStart(2, "0")}-01`,
    to: `2026-${String(i + 1).padStart(2, "0")}-28`,
  })),
  summary: { range: "Jan 2026 - Dec 2026", spending: "105.00", budget: "103.00", amount: "2.00", over: true, bar: { fill: 1000, split: 981 } },
  groups: [
    {
      kind: "expense",
      total: row(null, "", "Expenses", fig("103.00", "105.00", "-2.00")),
      rows: [row(3, "Food: ", "Groceries", fig("103.00", "80.00", "23.00"))],
      other: row(null, "", "Everything Else", fig("0.00", "25.00", "-25.00"), [row(9, "", "Fun", fig("0.00", "25.00", "-25.00"))]),
    },
    { kind: "income", total: row(null, "", "Income", zero), rows: [], other: null },
  ],
  total: row(null, "", "Total", fig("-103.00", "-105.00", "2.00")),
};

const budgetAnnual = vi.fn(() => ok(view));
const budgetSet = vi.fn(() => ok(null));
const budgetCopyMonth = vi.fn(() => ok(null));
const budgetYearCreate = vi.fn(() => ok(view.budget));

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      budgetAnnual: (...a: unknown[]) => budgetAnnual(...(a as [])),
      budgetSet: (...a: unknown[]) => budgetSet(...(a as [])),
      budgetCopyMonth: (...a: unknown[]) => budgetCopyMonth(...(a as [])),
      budgetYearCreate: (...a: unknown[]) => budgetYearCreate(...(a as [])),
      budgetList: () => ok([view.budget]),
    },
  };
});

import BudgetWindow from "./BudgetWindow.svelte";
import { budgetState } from "../../state/budget.svelte";
import { confirmState } from "../../state/confirm.svelte";
import { windowState } from "../../state/windows.svelte";

function open(): number {
  budgetState.open(1, 2026);
  return windowState.shown!;
}

beforeEach(() => {
  cleanup();
  vi.clearAllMocks();
  budgetState.list = [view.budget] as never;
});

describe("BudgetWindow, Annual View", () => {
  it("shows whole dollars, the path with the name bold, and Everything Else actual only", async () => {
    render(BudgetWindow, { id: open() });
    await screen.findByText("Groceries");
    expect(screen.getByText("Groceries").tagName).toBe("B");
    expect(screen.getByText("Food:")).toBeTruthy();
    const cell = screen.getByLabelText("Food: Groceries October budget") as HTMLInputElement;
    expect(cell.value).toBe("103");
    // Over budget says so in words, not only color.
    expect(screen.getByText(/⚠ \$2 OVER/)).toBeTruthy();
    const other = screen.getByText("Everything Else").closest("tr")!;
    expect(other.querySelectorAll("input")).toHaveLength(0);
    expect(other.textContent).toContain("25");
    expect(other.textContent).not.toContain("-25");
  });

  it("gives figure cells no class the window itself carries", async () => {
    // A shared class pulled the window's flex layout onto every budget
    // cell, stacking a row's months in one column.
    const { container } = render(BudgetWindow, { id: open() });
    const cell = await screen.findByLabelText("Food: Groceries October budget");
    const root = container.firstElementChild!;
    const td = cell.closest("td")!;
    const shared = [...td.classList].filter((c) => !c.startsWith("svelte-") && root.classList.contains(c));
    expect(shared).toEqual([]);
  });

  it("offers a row's copy icon only where budgets are shown", async () => {
    const id = open();
    render(BudgetWindow, { id });
    await screen.findByText("Groceries");
    expect(screen.getByRole("button", { name: "Copy a month across this row" })).toBeTruthy();
    budgetState.win(id)!.show = "actual";
    await waitFor(() => expect(screen.queryByRole("button", { name: "Copy a month across this row" })).toBeNull());    budgetState.win(id)!.show = "details";
  });

  it("edits a month's budget as entered and saves it on blur", async () => {
    render(BudgetWindow, { id: open() });
    const cell = (await screen.findByLabelText("Food: Groceries October budget")) as HTMLInputElement;
    await fireEvent.focus(cell);
    expect(cell.value).toBe("102.50");
    await fireEvent.input(cell, { target: { value: "120" } });
    await fireEvent.blur(cell);
    await waitFor(() => expect(budgetSet).toHaveBeenCalledWith(1, 3, 2026, 10, "120.00"));
  });

  it("copies a month from its heading's menu", async () => {
    render(BudgetWindow, { id: open() });
    await fireEvent.click(await screen.findByRole("button", { name: "March budget copy options" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Copy March budget to all future months in 2026" }));
    await waitFor(() => expect(budgetCopyMonth).toHaveBeenCalledWith(1, 2026, 3, "future", null));
  });

  it("asks before moving to a year the budget lacks", async () => {
    render(BudgetWindow, { id: open() });
    await screen.findByText("Groceries");
    const choose = vi.spyOn(confirmState, "choose").mockResolvedValue("Copy 2026's budget");
    await fireEvent.click(screen.getByRole("button", { name: "Next year" }));
    await waitFor(() => expect(budgetYearCreate).toHaveBeenCalledWith(1, 2027, 2026));
    expect(choose).toHaveBeenCalledWith("No budget for 2027. Create one?", ["Copy 2026's budget", "Copy only the categories"]);
    choose.mockResolvedValue(null);
    budgetYearCreate.mockClear();
    await fireEvent.click(screen.getByRole("button", { name: "Previous year" }));
    expect(budgetYearCreate).not.toHaveBeenCalled();
  });
});
