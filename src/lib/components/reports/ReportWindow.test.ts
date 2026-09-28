import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const run = vi.hoisted(() => vi.fn());
const create = vi.hoisted(() => vi.fn());
const update = vi.hoisted(() => vi.fn());
const goTo = vi.hoisted(() => vi.fn());

const defaults = (kind: string) => ({
  kind,
  title: kind === "capital_gains" ? "Capital Gains" : "Itemized Categories",
  range: { preset: "last_year", from: null, to: null },
  subtotal: "term",
  interval: "none",
  sort: "date",
  sort_desc: false,
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

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      reportDefaults: (kind: string) => Promise.resolve(defaults(kind)),
      reportRun: (s: unknown) => run(s),
      reportRange: () => ok({ from: "2025-01-01", to: "2025-12-31" }),
      reportColumns: () => Promise.resolve([]),
      savedReportCreate: (name: string, s: unknown) => create(name, s),
      savedReportUpdate: (id: number, name: string, s: unknown) => update(id, name, s),
      savedReportList: () => ok([]),
    },
  };
});
vi.mock("../../state/register.svelte", () => ({
  registerState: { goToTransaction: (...a: unknown[]) => goTo(...a), setFilters: vi.fn() },
}));

import ReportWindow from "./ReportWindow.svelte";
import { confirmState } from "../../state/confirm.svelte";
import { listsState } from "../../state/lists.svelte";
import { reportState, type ReportInstance } from "../../state/reports.svelte";
import { viewState } from "../../state/view.svelte";
import { windowState } from "../../state/windows.svelte";

let inst: ReportInstance;
const show = () => render(ReportWindow, { inst });

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
  windowState.reset();
  viewState.reset();
  listsState.today = "2026-09-27";
  run.mockImplementation(() => ok(report));
  create.mockImplementation((name: string, settings: unknown) => ok({ id: 1, name, settings }));
  update.mockImplementation((id: number, name: string, settings: unknown) => ok({ id, name, settings }));
  inst = await reportState.open("capital_gains");
});

describe("Report window", () => {
  it("shows the heading, dates, and rows with totals", async () => {
    show();
    expect(await screen.findByRole("heading", { name: "Capital Gains - Last year" })).toBeTruthy();
    expect(await screen.findByText("01/01/2025 through 12/31/2025")).toBeTruthy();
    expect(screen.getByText("Total LONG TERM")).toBeTruthy();
    expect(screen.getAllByText("11,237.14")).toHaveLength(3);
  });

  it("collapses a group to one line with its total", async () => {
    show();
    await fireEvent.click(await screen.findByRole("button", { name: "Collapse LONG TERM" }));
    expect(screen.queryByText("Total LONG TERM")).toBeNull();
    expect(screen.queryByText("Vanguard Cons Discretionary")).toBeNull();
    expect(screen.getAllByText("11,237.14")).toHaveLength(2);
    await fireEvent.click(screen.getByRole("button", { name: "Expand All" }));
    expect(screen.getByText("Total LONG TERM")).toBeTruthy();
  });

  it("a figure opens the transaction behind it", async () => {
    show();
    const figs = await screen.findAllByRole("button", { name: "11,237.14" });
    await fireEvent.click(figs[0]);
    await waitFor(() => expect(goTo).toHaveBeenCalledWith(3, 42, "2025-12-15"));
    expect(viewState.current).toBe("account");
  });

  it("the quick date range reruns the report", async () => {
    show();
    const select = await screen.findByRole("combobox", { name: "Date range" });
    await fireEvent.change(select, { target: { value: "year_to_date" } });
    await waitFor(() =>
      expect(run).toHaveBeenLastCalledWith(
        expect.objectContaining({ range: { preset: "year_to_date", from: null, to: null } }),
      ),
    );
  });

  it("saves under a name", async () => {
    show();
    await fireEvent.click(await screen.findByRole("button", { name: "Save…" }));
    const name = screen.getByRole("textbox", { name: "Name" });
    await fireEvent.input(name, { target: { value: "My gains" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(create).toHaveBeenCalledWith("My gains", expect.objectContaining({ kind: "capital_gains" })));
    expect(inst.saved?.name).toBe("My gains");
  });
  it("capital gains subtotals from the toolbar", async () => {
    show();
    const select = await screen.findByRole("combobox", { name: "Subtotal by:" });
    const labels = Array.from((select as HTMLSelectElement).options).map((o) => o.text);
    expect(labels).toEqual(["Short vs. long-term", "Month", "Quarter", "Year", "Account", "Security", "Don't subtotal"]);
    await fireEvent.change(select, { target: { value: "security" } });
    await waitFor(() => expect(run).toHaveBeenLastCalledWith(expect.objectContaining({ subtotal: "security" })));
    expect(screen.queryByRole("combobox", { name: "Sort by:" })).toBeNull();
  });

  it("custom dates open a dialog and apply its range", async () => {
    show();
    const select = await screen.findByRole("combobox", { name: "Date range" });
    await fireEvent.change(select, { target: { value: "custom" } });
    expect((select as HTMLSelectElement).value).toBe("last_year");
    const from = screen.getByRole("textbox", { name: "From" });
    const to = screen.getByRole("textbox", { name: "To" });
    await fireEvent.input(from, { target: { value: "03/01/2025" } });
    await fireEvent.change(from);
    await fireEvent.input(to, { target: { value: "02/01/2025" } });
    await fireEvent.change(to);
    await fireEvent.click(screen.getByRole("button", { name: "OK" }));
    expect(screen.getByRole("alert").textContent).toContain("From must be on or before To");
    await fireEvent.input(to, { target: { value: "06/30/2025" } });
    await fireEvent.change(to);
    await fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() =>
      expect(run).toHaveBeenLastCalledWith(
        expect.objectContaining({ range: { preset: "custom", from: "2025-03-01", to: "2025-06-30" } }),
      ),
    );
    expect(screen.getByRole("button", { name: "Change Dates…" })).toBeTruthy();
  });
});

describe("Itemized report toolbar", () => {
  const itemized = {
    ...report,
    kind: "itemized_categories",
    title: "Itemized Categories",
    columns: [
      { id: "date", label: "Date", kind: "date", from: null, to: null },
      { id: "account", label: "Account", kind: "text", from: null, to: null },
      { id: "num", label: "Num", kind: "text", from: null, to: null },
      { id: "amount", label: "Amount", kind: "money", from: null, to: null },
    ],
    rows: [],
  };

  beforeEach(async () => {
    run.mockImplementation(() => ok(itemized));
    inst = await reportState.open("itemized_categories");
  });

  it("sorts from the Sort by dropdown", async () => {
    show();
    const select = await screen.findByRole("combobox", { name: "Sort by:" });
    const labels = Array.from((select as HTMLSelectElement).options).map((o) => o.text);
    expect(labels).toEqual(["Date/Account", "Account/Date", "Amount"]);
    await fireEvent.change(select, { target: { value: "account_date" } });
    await waitFor(() =>
      expect(run).toHaveBeenLastCalledWith(expect.objectContaining({ sort: "account_date", sort_desc: false })),
    );
  });

  it("column headings sort, and a second click reverses", async () => {
    show();
    await fireEvent.click(await screen.findByRole("button", { name: /^Num/ }));
    await waitFor(() => expect(run).toHaveBeenLastCalledWith(expect.objectContaining({ sort: "num", sort_desc: false })));
    await fireEvent.click(await screen.findByRole("button", { name: /^Num/ }));
    await waitFor(() => expect(run).toHaveBeenLastCalledWith(expect.objectContaining({ sort: "num", sort_desc: true })));
    expect(screen.getByRole("columnheader", { name: /Num/ }).getAttribute("aria-sort")).toBe("descending");
    expect(screen.queryByRole("button", { name: /^Amount/ })).toBeNull();
  });
});

describe("Report windows", () => {
  const withChart = {
    ...report,
    chart: { dates: ["2025-12-31"], series: [], ticks: [], zero: 0 },
  };

  it("opens in a window on top, and reruns each time it is shown", async () => {
    expect(windowState.shown).toBe(inst.id);
    show();
    await waitFor(() => expect(run).toHaveBeenCalledTimes(1));
    cleanup();
    show();
    await waitFor(() => expect(run).toHaveBeenCalledTimes(2));
  });

  it("hides and opens the graph and the report", async () => {
    run.mockImplementation(() => ok(withChart));
    show();
    await fireEvent.click(await screen.findByRole("button", { name: "Hide Graph" }));
    expect(screen.queryByRole("img")).toBeNull();
    expect(screen.getByRole("button", { name: "Open Graph" })).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Hide Report" }));
    expect(screen.queryByRole("table")).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Open Report" }));
    expect(screen.getByRole("table")).toBeTruthy();
  });

  it("no fold buttons without a graph", async () => {
    show();
    await screen.findByRole("table");
    expect(screen.queryByRole("button", { name: "Hide Report" })).toBeNull();
  });

  it("closing an unchanged report does not ask", async () => {
    expect(await windowState.close(inst.id)).toBe(true);
    expect(confirmState.message).toBeNull();
    expect(windowState.wins).toHaveLength(0);
  });

  it("closing a changed report asks to save; Don't Save closes it", async () => {
    await inst.apply({ ...inst.settings, subtotal: "year" });
    const closing = windowState.close(inst.id);
    await waitFor(() => expect(confirmState.choices).toEqual(["Save", "Don't Save"]));
    confirmState.answer("Don't Save");
    expect(await closing).toBe(true);
    expect(reportState.get(inst.id)).toBeUndefined();
  });

  it("Save on close names a new report, saves, then closes", async () => {
    show();
    await inst.apply({ ...inst.settings, subtotal: "year" });
    viewState.navigate("dashboard");
    const closing = windowState.close(inst.id);
    await waitFor(() => expect(confirmState.message).not.toBeNull());
    confirmState.answer("Save");
    expect(await closing).toBe(false);
    expect(windowState.shown).toBe(inst.id);
    const name = await screen.findByRole("textbox", { name: "Name" });
    await fireEvent.input(name, { target: { value: "Yearly gains" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(create).toHaveBeenCalledWith("Yearly gains", expect.objectContaining({ subtotal: "year" })));
    await waitFor(() => expect(windowState.wins).toHaveLength(0));
  });

  it("Save on close of a saved report updates it", async () => {
    await inst.save("Mine", false);
    await inst.apply({ ...inst.settings, subtotal: "year" });
    expect(inst.dirty).toBe(true);
    const closing = windowState.close(inst.id);
    await waitFor(() => expect(confirmState.message).not.toBeNull());
    confirmState.answer("Save");
    expect(await closing).toBe(true);
    expect(update).toHaveBeenCalledWith(1, "Mine", expect.objectContaining({ subtotal: "year" }));
  });

  it("Cancel keeps it open", async () => {
    await inst.apply({ ...inst.settings, cents: false });
    const closing = windowState.close(inst.id);
    await waitFor(() => expect(confirmState.message).not.toBeNull());
    confirmState.answer(false);
    expect(await closing).toBe(false);
    expect(windowState.wins).toHaveLength(1);
  });

  it("a category figure opens Itemized Categories in a second window", async () => {
    run.mockImplementation(() =>
      ok({
        ...report,
        kind: "income_expense",
        columns: [{ id: "amount", label: "Amount", kind: "money", from: "2025-01-01", to: "2025-03-31" }],
        rows: [{ kind: "detail", label: "Food", cells: ["-5.00"], drill: { kind: "category", category: 7 }, children: [] }],
      }),
    );
    show();
    await fireEvent.click(await screen.findByRole("button", { name: "-5.00" }));
    await waitFor(() => expect(windowState.wins).toHaveLength(2));
    const second = reportState.current!;
    expect(second.id).not.toBe(inst.id);
    expect(second.settings).toMatchObject({
      kind: "itemized_categories",
      range: { preset: "custom", from: "2025-01-01", to: "2025-03-31" },
      categories: [7],
    });
  });
});
