import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

const fig = (budget: string, actual: string, balance: string) => ({ budget, actual, balance });
function row(category: number | null, prefix: string, name: string, f: ReturnType<typeof fig>, more = {}) {
  return {
    category,
    prefix,
    name,
    figures: f,
    entered: f.budget,
    standing: "left",
    amount: "23.00",
    bar: { fill: 800, split: null },
    counted: category === null ? [9] : [category],
    children: [],
    ...more,
  };
}
const over = row(4, "", "Dining", fig("100.00", "150.00", "-50.00"), { standing: "over", amount: "50.00", bar: { fill: 1000, split: 667 } });
const view = (more = {}) => ({
  budget: { id: 1, name: "Household", categories: [3], years: [2026] },
  period: "monthly",
  at: { year: 2026, index: 10 },
  prev: { year: 2026, index: 9 },
  next: { year: 2026, index: 11 },
  title: "October 2026",
  missing_year: null,
  editable: true,
  from: "2026-10-01",
  to: "2026-10-31",
  summary: { range: "Oct 2026", spending: "230.00", budget: "203.00", amount: "27.00", over: true, bar: { fill: 1000, split: 883 } },
  groups: [
    {
      kind: "expense",
      total: row(null, "", "Expenses", fig("203.00", "230.00", "-27.00")),
      rows: [row(3, "Food: ", "Groceries", fig("103.00", "80.00", "23.00")), over],
      other: null,
    },
  ],
  total: row(null, "", "Total", fig("-203.00", "-230.00", "27.00"), { bar: null }),
  ...more,
});

const budgetGraph = vi.fn((..._a: unknown[]) => ok(view()));
const budgetSetPeriod = vi.fn((..._a: unknown[]) => ok(null));
const budgetYearCreate = vi.fn((..._a: unknown[]) => ok(null));

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      budgetGraph: (...a: unknown[]) => budgetGraph(...a),
      budgetSetPeriod: (...a: unknown[]) => budgetSetPeriod(...a),
      budgetYearCreate: (...a: unknown[]) => budgetYearCreate(...a),
      budgetList: () => ok([]),
    },
  };
});

import BudgetWindow from "./BudgetWindow.svelte";
import { budgetState } from "../../state/budget.svelte";
import { confirmState } from "../../state/confirm.svelte";
import { windowState } from "../../state/windows.svelte";

function open() {
  budgetState.open(1, 2026);
  const id = windowState.shown!;
  const w = budgetState.win(id)!;
  w.view = "graph";
  w.period = "monthly";
  w.at = null;
  render(BudgetWindow, { id });
  return w;
}

beforeEach(() => {
  cleanup();
  vi.clearAllMocks();
  budgetGraph.mockImplementation(() => ok(view()));
  budgetState.list = [view().budget] as never;
});

describe("Graph View (BUD-060)", () => {
  it("says each row's standing in words, not color alone", async () => {
    open();
    expect((await screen.findAllByText("23 left")).length).toBeGreaterThan(0);
    expect(screen.getByText("⚠ 50 over")).toBeTruthy();
    expect(screen.getByLabelText("Spending summary")).toBeTruthy();
  });

  it("moves between periods, and a new period starts on the current one", async () => {
    const w = open();
    fireEvent.click(await screen.findByRole("button", { name: "Previous period" }));
    await waitFor(() => expect(w.at).toEqual({ year: 2026, index: 9 }));
    const select = screen.getByLabelText("Period") as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: "quarterly" } });
    expect(w.period).toBe("quarterly");
    expect(w.at).toBeNull();
  });

  it("spreads an edited amount over the period", async () => {
    open();
    const box = (await screen.findByLabelText("Food: Groceries budget for the period")) as HTMLInputElement;
    await fireEvent.focus(box);
    await fireEvent.input(box, { target: { value: "300" } });
    await fireEvent.blur(box);
    await waitFor(() => expect(budgetSetPeriod).toHaveBeenCalled());
    expect(budgetSetPeriod.mock.calls[0]).toEqual([1, 3, "monthly", { year: 2026, index: 10 }, "300.00"]);
  });

  it("leaves the budget alone when nothing was typed", async () => {
    open();
    const box = await screen.findByLabelText("Food: Groceries budget for the period");
    await fireEvent.focus(box);
    await fireEvent.blur(box);
    expect(budgetSetPeriod).not.toHaveBeenCalled();
  });

  it("asks before moving to a year the budget lacks, and stays on cancel (BUD-050)", async () => {
    budgetGraph.mockImplementation(() => ok(view({ prev: { year: 2025, index: 12 } })));
    const choose = vi.spyOn(confirmState, "choose").mockResolvedValue(null);
    const w = open();
    fireEvent.click(await screen.findByRole("button", { name: "Previous period" }));
    await waitFor(() => expect(choose).toHaveBeenCalled());
    expect(w.at).toBeNull();
    expect(budgetYearCreate).not.toHaveBeenCalled();
  });

  it("shows budgets as text when the period can't be edited", async () => {
    budgetGraph.mockImplementation(() => ok(view({ editable: false, at: null, title: "" })));
    open();
    await screen.findAllByText("23 left");
    expect(screen.queryByLabelText("Food: Groceries budget for the period")).toBeNull();
    expect(screen.queryByRole("button", { name: "Previous period" })).toBeNull();
  });
});
