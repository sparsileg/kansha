import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const run = vi.hoisted(() => vi.fn());
const create = vi.hoisted(() => vi.fn());
const update = vi.hoisted(() => vi.fn());
const goTo = vi.hoisted(() => vi.fn());
const savePdf = vi.hoisted(() => vi.fn());

const defaults = (kind: string) => ({
  kind,
  title: kind === "capital_gains" ? "Capital Gains" : "Itemized Categories",
  range: { preset: "last_year", from: null, to: null },
  subtotal: "term",
  tax_group: "category",
  interval: "none",
  sort: "date",
  sort_desc: false,
  hidden_columns: [],
  cents: true,
  totals_only: false,
  totals_on_heading: null,
  show_zero: false,
  transfers: true,
  cash_flow: false,
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
      reportPeriodChoices: (r: { from: string | null }) => {
        const years = ["2026", "2025", "2024", "2023", "2022"];
        if (r.from && !years.includes(r.from.slice(0, 4))) years.push(r.from.slice(0, 4));
        return ok(years.map((y) => ({ from: `${y}-01-01`, to: `${y}-12-31`, label: y })));
      },
      reportColumns: () => Promise.resolve([]),
      savedReportCreate: (name: string, s: unknown) => create(name, s),
      savedReportUpdate: (id: number, name: string, s: unknown) => update(id, name, s),
      savedReportList: () => ok([]),
      reportSavePdf: (title: string, orientation: string) => savePdf(title, orientation),
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
import { statusState } from "../../state/status.svelte";
import { viewState } from "../../state/view.svelte";
import { investState } from "../../state/invest.svelte";
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
  totals_on_heading: false,
  compact: false,
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
    await waitFor(() => expect(goTo).toHaveBeenCalledWith(3, 42));
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
  it("Save PDF asks the orientation, saves, and says where", async () => {
    savePdf.mockImplementation(() => ok("/home/u/Downloads/Capital Gains 2026-09-27.pdf"));
    show();
    await fireEvent.click(await screen.findByRole("button", { name: "Save PDF…" }));
    const dialog = screen.getByRole("dialog", { name: "Save PDF" });
    expect((within(dialog).getByRole("radio", { name: "Portrait" }) as HTMLInputElement).checked).toBe(true);
    expect(within(dialog).queryByRole("button", { name: /Print/ })).toBeNull();
    await fireEvent.click(within(dialog).getByRole("radio", { name: "Landscape" }));
    await fireEvent.click(within(dialog).getByRole("button", { name: "Save PDF" }));
    await waitFor(() => expect(savePdf).toHaveBeenCalledWith("Capital Gains - Last year", "landscape"));
    expect(screen.queryByRole("dialog", { name: "Save PDF" })).toBeNull();
    await waitFor(() => expect(statusState.message?.text).toMatch(/^Saved to .*Capital Gains 2026-09-27\.pdf$/));
    // The choice is remembered for the next PDF.
    await fireEvent.click(screen.getByRole("button", { name: "Save PDF…" }));
    const again = screen.getByRole("dialog", { name: "Save PDF" });
    expect((within(again).getByRole("radio", { name: "Landscape" }) as HTMLInputElement).checked).toBe(true);
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

  it("Tax Summary: Subtotal by and Sort by on the bar (RPT-140)", async () => {
    run.mockImplementation(() => ok({ ...itemized, kind: "tax_summary", title: "Tax Summary" }));
    inst = await reportState.open("tax_summary");
    show();
    const group = await screen.findByRole("combobox", { name: "Subtotal by:" });
    const labels = Array.from((group as HTMLSelectElement).options).map((o) => o.text);
    expect(labels).toEqual(["Category", "Tax line", "Account", "Payee", "Tag", "Month", "Quarter", "Year", "Don't subtotal"]);
    expect((group as HTMLSelectElement).value).toBe("category");
    await fireEvent.change(group, { target: { value: "tax_line" } });
    await waitFor(() => expect(run).toHaveBeenLastCalledWith(expect.objectContaining({ tax_group: "tax_line" })));
    const sort = screen.getByRole("combobox", { name: "Sort by:" });
    await fireEvent.change(sort, { target: { value: "amount" } });
    await waitFor(() =>
      expect(run).toHaveBeenLastCalledWith(expect.objectContaining({ tax_group: "tax_line", sort: "amount", sort_desc: false })),
    );
  });

  it("Tax Schedule: Monthly/Quarterly/Yearly after all dates; Yearly picks a year (RPT-040)", async () => {
    run.mockImplementation(() => ok({ ...itemized, kind: "tax_schedule", title: "Tax Schedule" }));
    inst = await reportState.open("tax_schedule");
    show();
    const select = (await screen.findByRole("combobox", { name: "Date range" })) as HTMLSelectElement;
    const texts = Array.from(select.options).map((o) => o.text);
    expect(texts.slice(0, 5)).toEqual(["Include all dates", "Monthly", "Quarterly", "Yearly", "Month to date"]);
    expect(texts).not.toContain("This month");
    expect(screen.queryByRole("combobox", { name: "Period" })).toBeNull();

    await fireEvent.change(select, { target: { value: "yearly" } });
    await waitFor(() =>
      expect(run).toHaveBeenLastCalledWith(
        expect.objectContaining({ range: { preset: "yearly", from: "2026-01-01", to: null } }),
      ),
    );
    const period = (await screen.findByRole("combobox", { name: "Period" })) as HTMLSelectElement;
    expect(Array.from(period.options).map((o) => o.text)).toEqual(["2026", "2025", "2024", "2023", "2022"]);
    expect(await screen.findByRole("heading", { name: /- 2026$/ })).toBeTruthy();
    await fireEvent.change(period, { target: { value: "2024-01-01" } });
    await waitFor(() =>
      expect(run).toHaveBeenLastCalledWith(
        expect.objectContaining({ range: { preset: "yearly", from: "2024-01-01", to: null } }),
      ),
    );
    expect(await screen.findByRole("heading", { name: /- 2024$/ })).toBeTruthy();
  });

  it("Customize: Yearly shows the year list; OK applies the year picked", async () => {
    run.mockImplementation(() => ok({ ...itemized, kind: "tax_schedule", title: "Tax Schedule" }));
    inst = await reportState.open("tax_schedule");
    show();
    await fireEvent.click(await screen.findByRole("button", { name: "Customize…" }));
    const dialog = await screen.findByRole("dialog");
    await fireEvent.change(within(dialog).getByRole("combobox", { name: "Date range" }), { target: { value: "yearly" } });
    const period = (await within(dialog).findByRole("combobox", { name: "Period" })) as HTMLSelectElement;
    await waitFor(() => expect(period.value).toBe("2026-01-01"));
    await fireEvent.change(period, { target: { value: "2023-01-01" } });
    await fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(() =>
      expect(run).toHaveBeenLastCalledWith(
        expect.objectContaining({ range: { preset: "yearly", from: "2023-01-01", to: null } }),
      ),
    );
  });

  it("other reports have no Monthly/Quarterly/Yearly", async () => {
    show();
    const select = (await screen.findByRole("combobox", { name: "Date range" })) as HTMLSelectElement;
    const texts = Array.from(select.options).map((o) => o.text);
    expect(texts).not.toContain("Monthly");
    expect(texts).not.toContain("This year");
    expect(texts[0]).toBe("Include all dates");
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

describe("Tax Schedule (compact)", () => {
  const detail = (cells: string[]) => ({
    kind: "detail",
    label: "",
    cells,
    drill: { kind: "txn", account: 1, txn: 9, date: "2025-04-01" },
    children: [],
  });
  const tax = {
    ...report,
    kind: "tax_schedule",
    title: "Tax Schedule",
    columns: [
      { id: "date", label: "Date", kind: "date", from: null, to: null },
      { id: "account", label: "Account", kind: "text", from: null, to: null },
      { id: "description", label: "Description", kind: "text", from: null, to: null },
      { id: "category", label: "Category", kind: "text", from: null, to: null },
      { id: "amount", label: "Amount", kind: "money", from: null, to: null },
    ],
    rows: [
      {
        kind: "section",
        label: "1099-R",
        cells: ["", "", "", "", "5000.00"],
        drill: null,
        children: [
          {
            kind: "group",
            label: "Total IRA taxable distrib.",
            cells: ["", "", "", "", "5000.00"],
            drill: null,
            children: [detail(["2025-04-01", "Checking", "Roth conversion to Vanguard", "[IRA]", "5000.00"])],
          },
        ],
      },
    ],
    totals_on_heading: true,
    compact: true,
  };

  beforeEach(async () => {
    run.mockImplementation(() => ok(tax));
    inst = await reportState.open("tax_schedule");
  });

  it("has no label column; dates sit under the headings, which carry the totals", async () => {
    show();
    await screen.findByText("Roth conversion to Vanguard");
    const table = document.querySelector("table.report.compact")!;
    expect(table).toBeTruthy();
    expect(within(table as HTMLElement).getAllByRole("columnheader").map((h) => h.textContent?.trim())).toEqual([
      "Date",
      "Account",
      "Description",
      "Category",
      "Amount",
    ]);
    const rows = table.querySelectorAll("tbody tr");
    expect(rows).toHaveLength(3);
    // The form row is shaded (class section); labels span up to Amount.
    expect(rows[0].classList.contains("section")).toBe(true);
    expect(rows[0].querySelector("td.label")?.getAttribute("colspan")).toBe("4");
    expect(rows[0].textContent).toContain("5,000.00");
    expect(rows[1].textContent).toContain("5,000.00");
    expect(rows[2].querySelectorAll("td")).toHaveLength(5);
    expect(rows[2].querySelector("td")?.textContent?.trim()).toBe("04/01/2025");
    // Negative amounts get the negative color; others do not.
    expect(rows[2].querySelector("td.num")?.classList.contains("neg")).toBe(false);
    // The Date heading lines up with the dates.
    const date = (table.querySelector("thead th") as HTMLElement).style.paddingLeft;
    expect(date).toBe("3em");
    expect(date).toBe((rows[2].querySelector("td") as HTMLElement).style.paddingLeft);
    expect(screen.queryByText(/^Total Total/)).toBeNull();
    // Cut text keeps its full text on hover.
    expect(screen.getByText("Roth conversion to Vanguard").closest("td")?.getAttribute("title")).toBe(
      "Roth conversion to Vanguard",
    );
  });

  it("Save PDF prints one table per page, each with the headings, then goes back", async () => {
    let printed = 0;
    savePdf.mockImplementation(() => {
      printed = document.querySelectorAll(".pages .paper-page table thead").length;
      return ok("/home/u/Downloads/Tax Schedule.pdf");
    });
    show();
    await screen.findByText("Roth conversion to Vanguard");
    await fireEvent.click(screen.getByRole("button", { name: "Save PDF…" }));
    await fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Save PDF" }));
    await waitFor(() => expect(savePdf).toHaveBeenCalled());
    expect(printed).toBe(1);
    await waitFor(() => expect(document.querySelector(".pages")).toBeNull());
  });

  it("negative amounts are marked for the negative color, sign kept", async () => {
    const neg = {
      ...tax,
      rows: [
        {
          kind: "section",
          label: "Schedule A",
          cells: ["", "", "", "", "-1200.00"],
          drill: null,
          children: [detail(["2025-03-15", "Checking", "County", "Tax:Real Estate", "-1200.00"])],
        },
      ],
    };
    run.mockImplementation(() => ok(neg));
    inst = await reportState.open("tax_schedule");
    show();
    await screen.findByText("County");
    const cells = [...document.querySelectorAll("table.report.compact tbody td.num")];
    expect(cells.map((c) => c.textContent?.trim())).toEqual(["-1,200.00", "-1,200.00"]);
    expect(cells.every((c) => c.classList.contains("neg"))).toBe(true);
    expect(document.querySelector("td.neg:not(.num)")).toBeNull();
  });

  it("a column turned back on shows up (widths measured again)", async () => {
    const without = (r: typeof tax) => ({
      ...r,
      columns: r.columns.filter((c) => c.id !== "account"),
      rows: r.rows.map((s) => ({
        ...s,
        cells: s.cells.slice(1),
        children: s.children.map((g) => ({
          ...g,
          cells: g.cells.slice(1),
          children: g.children.map((d) => ({ ...d, cells: [d.cells[0], ...d.cells.slice(2)] })),
        })),
      })),
    });
    run.mockImplementation(() => ok(without(tax)));
    inst = await reportState.open("tax_schedule");
    show();
    await screen.findByText("Roth conversion to Vanguard");
    expect(screen.queryByRole("columnheader", { name: "Account" })).toBeNull();
    run.mockImplementation(() => ok(tax));
    await fireEvent.click(screen.getByRole("button", { name: "Customize…" }));
    await fireEvent.click(await screen.findByRole("button", { name: "OK" }));
    await screen.findByRole("columnheader", { name: "Account" });
    await screen.findByText("Checking");
    await waitFor(() => expect(document.querySelectorAll("table.report.compact col")).toHaveLength(5));
  });

  it("Totals on group heading shows the report's default and saves a change", async () => {
    show();
    await screen.findByText("Roth conversion to Vanguard");
    await fireEvent.click(screen.getByRole("button", { name: "Customize…" }));
    const box = (await screen.findByRole("checkbox", { name: "Totals on group heading" })) as HTMLInputElement;
    expect(box.checked).toBe(true);
    await fireEvent.click(box);
    await fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() => expect(run).toHaveBeenLastCalledWith(expect.objectContaining({ totals_on_heading: false })));
  });
});

describe("Report windows", () => {
  const withChart = {
    ...report,
    chart: { dates: ["2025-12-31"], labels: [], series: [], ticks: [], zero: 0, x_unit: "month" },
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
    // The graph is screen-only: printing leaves it out.
    const graph = await screen.findByRole("img");
    expect(graph.closest(".no-print")).not.toBeNull();
    expect(screen.getByRole("table").closest(".no-print")).toBeNull();
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
    viewState.navigate("insights");
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

  it("a saved report is headed by its name", async () => {
    expect(inst.heading).toBe("Capital Gains - Last year");
    await inst.save("Gains Last Year", false);
    expect(inst.heading).toBe("Gains Last Year");
    show();
    expect(await screen.findByRole("heading", { name: "Gains Last Year" })).toBeTruthy();
  });

  it("the menu replaces its open copy of the report, asking first if changed", async () => {
    await inst.apply({ ...inst.settings, subtotal: "year" });
    inst.expandAll();
    const again = reportState.open("capital_gains");
    await waitFor(() => expect(confirmState.choices).toEqual(["Save", "Don't Save"]));
    confirmState.answer("Don't Save");
    expect(await again).toBe(inst);
    expect(windowState.wins).toHaveLength(1);
    expect(inst.settings.subtotal).toBe("term");
    expect(inst.dirty).toBe(false);
    // Unchanged now: no question the next time.
    expect(await reportState.open("capital_gains")).toBe(inst);
    expect(confirmState.message).toBeNull();
  });

  it("Cancel keeps the menu copy's changes", async () => {
    await inst.apply({ ...inst.settings, subtotal: "year" });
    const again = reportState.open("capital_gains");
    await waitFor(() => expect(confirmState.message).not.toBeNull());
    confirmState.answer(false);
    expect(await again).toBe(inst);
    expect(inst.settings.subtotal).toBe("year");
  });

  it("Save names the changed menu copy, which stays; a fresh copy opens", async () => {
    show();
    await inst.apply({ ...inst.settings, subtotal: "year" });
    const again = reportState.open("capital_gains");
    await waitFor(() => expect(confirmState.message).not.toBeNull());
    confirmState.answer("Save");
    await again;
    const name = await screen.findByRole("textbox", { name: "Name" });
    await fireEvent.input(name, { target: { value: "Yearly gains" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(windowState.wins).toHaveLength(2));
    expect(inst.heading).toBe("Yearly gains");
    expect(reportState.current!.id).not.toBe(inst.id);
    expect(reportState.current!.settings.subtotal).toBe("term");
  });

  it("the menu leaves saved reports alone; a saved report replaces its own copy", async () => {
    const saved = await inst.save("Mine", false);
    const menu = await reportState.open("capital_gains");
    expect(menu).not.toBe(inst);
    expect(windowState.wins).toHaveLength(2);
    await inst.apply({ ...inst.settings, subtotal: "year" });
    const again = reportState.openSaved(saved);
    await waitFor(() => expect(confirmState.message).not.toBeNull());
    confirmState.answer("Save");
    expect(await again).toBe(inst);
    expect(update).toHaveBeenCalledWith(1, "Mine", expect.objectContaining({ subtotal: "year" }));
    // Reopened with what was just saved, not the older settings.
    expect(inst.settings.subtotal).toBe("year");
    expect(inst.dirty).toBe(false);
    expect(windowState.wins).toHaveLength(2);
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
    // The menu opens its own copy; the drill-down stays.
    await reportState.open("itemized_categories");
    expect(windowState.wins).toHaveLength(3);
  });
});

describe("Investing reports", () => {
  const allocation = {
    ...report,
    kind: "asset_allocation",
    title: "Asset Allocation",
    as_of: true,
    columns: [
      { id: "value", label: "Market Value", kind: "money", from: null, to: null },
      { id: "percent", label: "Percent", kind: "percent", from: null, to: null },
    ],
    rows: [
      { kind: "detail", label: "Cash", cells: ["12920.00", "78.07"], drill: { kind: "asset_class", asset_class: "cash" }, children: [] },
      { kind: "detail", label: "US equity", cells: ["3630.00", "21.93"], drill: { kind: "asset_class", asset_class: "us_equity" }, children: [] },
      { kind: "total", label: "TOTAL", cells: ["16550.00", "100.00"], drill: null, children: [] },
    ],
    chart: {
      dates: [],
      labels: ["Cash", "US equity"],
      series: [{ name: "Market Value", style: "bar", values: ["12920.00", "3630.00"], pos: [7807, 2193] }],
      ticks: [{ label: "0", pos: 0 }, { label: "20K", pos: 10000 }],
      zero: 0,
    },
  };

  beforeEach(async () => {
    run.mockImplementation(() => ok(allocation));
    investState.securities = [
      { id: 1, name: "Total Stock Market", ticker: "VTI", asset_class: "us_equity" },
      { id: 2, name: "S&P 500", ticker: "VOO", asset_class: "us_equity" },
      { id: 3, name: "Bond Fund", ticker: "BND", asset_class: "bond" },
    ] as never;
    inst = await reportState.open("asset_allocation");
  });

  it("names the graph's bars by class and shows percents", async () => {
    show();
    const graph = await screen.findByRole("img", { name: /Market Value/ });
    expect(within(graph).getByText("US equity")).toBeTruthy();
    expect(screen.getByText("21.93%")).toBeTruthy();
  });

  it("a class opens Holdings limited to that class's securities", async () => {
    show();
    const before = windowState.wins.length;
    await fireEvent.click(await screen.findByRole("button", { name: "3,630.00" }));
    await waitFor(() => expect(windowState.wins).toHaveLength(before + 1));
    expect(reportState.current?.settings).toMatchObject({
      kind: "holdings",
      range: { preset: "custom", from: null, to: "2025-12-31" },
      securities: [1, 2],
      title: "Holdings: US equity",
    });
  });
});
