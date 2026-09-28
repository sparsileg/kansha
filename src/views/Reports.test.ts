import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const run = vi.hoisted(() => vi.fn());
const create = vi.hoisted(() => vi.fn());
const goTo = vi.hoisted(() => vi.fn());

const defaults = (kind: string) => ({
  kind,
  title: kind === "capital_gains" ? "Capital Gains" : "Itemized Categories",
  range: { preset: "last_year", from: null, to: null },
  subtotal: "term",
  interval: "none",
  sort: "date",
  hidden_columns: [],
  cents: true,
  totals_only: false,
  show_zero: false,
  transfers: true,
  accounts: null,
  categories: null,
  payees: null,
  securities: null,
  tags: null,
});

vi.mock("../lib/api", async (orig) => {
  const real = await orig<typeof import("../lib/api")>();
  return {
    ...real,
    commands: {
      reportDefaults: (kind: string) => Promise.resolve(defaults(kind)),
      reportRun: (s: unknown) => run(s),
      reportRange: () => ok({ from: "2025-01-01", to: "2025-12-31" }),
      reportColumns: () => Promise.resolve([]),
      savedReportCreate: (name: string, s: unknown) => create(name, s),
      savedReportList: () => ok([]),
    },
  };
});
vi.mock("../lib/state/register.svelte", () => ({
  registerState: { goToTransaction: (...a: unknown[]) => goTo(...a), setFilters: vi.fn() },
}));

import Reports from "./Reports.svelte";
import { listsState } from "../lib/state/lists.svelte";
import { reportState } from "../lib/state/reports.svelte";
import { viewState } from "../lib/state/view.svelte";

const report = {
  kind: "capital_gains",
  title: "Capital Gains",
  note: "",
  from: "2025-01-01",
  to: "2025-12-31",
  as_of: false,
  cents: true,
  columns: [
    { id: "security", label: "Security", kind: "text", from: null, to: null },
    { id: "sold", label: "Sold", kind: "date", from: null, to: null },
    { id: "gain", label: "Realized Gain/Loss", kind: "money", from: null, to: null },
  ],
  rows: [
    {
      kind: "group",
      label: "LONG TERM",
      cells: ["", "", "11237.14"],
      drill: null,
      children: [
        {
          kind: "detail",
          label: "",
          cells: ["Vanguard Cons Discretionary", "2025-12-15", "11237.14"],
          drill: { kind: "txn", account: 3, txn: 42, date: "2025-12-15" },
          children: [],
        },
      ],
    },
    { kind: "total", label: "OVERALL TOTAL", cells: ["", "", "11237.14"], drill: null, children: [] },
  ],
  chart: null,
};

beforeEach(async () => {
  cleanup();
  vi.clearAllMocks();
  viewState.reset();
  listsState.today = "2026-09-27";
  run.mockImplementation(() => ok(report));
  create.mockImplementation((name: string, settings: unknown) => ok({ id: 1, name, settings }));
  await reportState.open("capital_gains");
});

describe("Reports view", () => {
  it("shows the heading, dates, and rows with totals", async () => {
    render(Reports);
    expect(await screen.findByRole("heading", { name: "Capital Gains - Last year" })).toBeTruthy();
    expect(screen.getByText("01/01/2025 through 12/31/2025")).toBeTruthy();
    expect(screen.getByText("Total LONG TERM")).toBeTruthy();
    expect(screen.getAllByText("11,237.14")).toHaveLength(3);
  });

  it("collapses a group to one line with its total", async () => {
    render(Reports);
    await fireEvent.click(await screen.findByRole("button", { name: "Collapse LONG TERM" }));
    expect(screen.queryByText("Total LONG TERM")).toBeNull();
    expect(screen.queryByText("Vanguard Cons Discretionary")).toBeNull();
    expect(screen.getAllByText("11,237.14")).toHaveLength(2);
    await fireEvent.click(screen.getByRole("button", { name: "Expand All" }));
    expect(screen.getByText("Total LONG TERM")).toBeTruthy();
  });

  it("a figure opens the transaction behind it", async () => {
    render(Reports);
    const figs = await screen.findAllByRole("button", { name: "11,237.14" });
    await fireEvent.click(figs[0]);
    await waitFor(() => expect(goTo).toHaveBeenCalledWith(3, 42, "2025-12-15"));
    expect(viewState.current).toBe("account");
  });

  it("the quick date range reruns the report", async () => {
    render(Reports);
    const select = await screen.findByRole("combobox", { name: "Date range" });
    await fireEvent.change(select, { target: { value: "year_to_date" } });
    await waitFor(() =>
      expect(run).toHaveBeenLastCalledWith(
        expect.objectContaining({ range: { preset: "year_to_date", from: null, to: null } }),
      ),
    );
  });

  it("saves under a name", async () => {
    render(Reports);
    await fireEvent.click(await screen.findByRole("button", { name: "Save…" }));
    const name = screen.getByRole("textbox", { name: "Name" });
    await fireEvent.input(name, { target: { value: "My gains" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(create).toHaveBeenCalledWith("My gains", expect.objectContaining({ kind: "capital_gains" })));
    expect(reportState.saved?.name).toBe("My gains");
  });
});
