import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const chartCalls = vi.hoisted(() => vi.fn());
const update = vi.hoisted(() => vi.fn());
const chart = { dates: ["2026-01-02", "2026-06-30"], labels: [], series: [{ name: "Market Value", style: "line", values: ["1.00", "2.00"], pos: [5000, 10000] }], ticks: [], zero: 0, x_unit: "month" };
const sec = (id: number, name: string, ticker: string | null) => ({
  id, name, ticker, security_type: "mutual_fund", asset_class: "us_equity", cusip: null,
  default_lot_method: null, hidden: false, notes: "", created_at: "",
});

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      securityList: () => ok([sec(1, "Total Stock Market", "VTSAX"), sec(2, "Bond Fund", null)]),
      securityTransactions: (id: number) =>
        ok(id === 1 ? [{ account: 5, txn_id: 9, date: "2026-02-01", action: "buy", action_label: "Buy", quantity: "10", price: "100", commission: "0.00", amount: "-1000.00", memo: "", incoming: false, future: false }] : []),
      securityChart: (...a: unknown[]) => (chartCalls(...a), ok(chart)),
      securityUpdate: (...a: unknown[]) => (update(...a), ok(sec(1, "Total Market", "VTSAX"))),
    },
  };
});

import SecurityDetailsModal from "./SecurityDetailsModal.svelte";
import { investState } from "../../state/invest.svelte";
import { listsState } from "../../state/lists.svelte";
import { dateFormatState } from "../../state/dateformat.svelte";

beforeEach(async () => {
  chartCalls.mockClear();
  update.mockClear();
  dateFormatState.set("mdy");
  listsState.today = "2026-06-30";
  listsState.accounts = [{ id: 5, name: "Brokerage" }] as never;
  await investState.loadSecurities();
});

describe("Security Details", () => {
  it("shows the security clicked: its card, transactions, and a year's market value", async () => {
    render(SecurityDetailsModal, { security: 1, onclose: () => {} });
    const dlg = screen.getByRole("dialog", { name: "Security Details" });
    expect((within(dlg).getByRole("combobox", { name: "Security" }) as HTMLSelectElement).value).toBe("1");
    const info = within(dlg).getByRole("article", { name: "Security" });
    expect(info.textContent).toContain("VTSAX");
    expect(info.textContent).toContain("Mutual fund");
    await waitFor(() => expect(within(dlg).getByText("Brokerage")).toBeTruthy());
    expect(chartCalls).toHaveBeenLastCalledWith(1, "market_value", "year", null, null, true);
  });

  it("the graph follows its two dropdowns; Custom sends its dates", async () => {
    render(SecurityDetailsModal, { security: 1, onclose: () => {} });
    await fireEvent.change(screen.getByRole("combobox", { name: "Graph of" }), { target: { value: "price_history" } });
    await waitFor(() => expect(chartCalls).toHaveBeenLastCalledWith(1, "price_history", "year", null, null, true));
    const spans = [...(screen.getByRole("combobox", { name: "Interval" }) as HTMLSelectElement).options].map((o) => o.text);
    expect(spans).toEqual(["Week", "Month", "Three Months", "Year to Date", "Year", "2 Years", "5 Years", "Custom"]);
    await fireEvent.change(screen.getByRole("combobox", { name: "Interval" }), { target: { value: "custom" } });
    await waitFor(() => expect(chartCalls).toHaveBeenLastCalledWith(1, "price_history", "custom", "2026-01-02", "2026-06-30", true));
  });

  it("Fit graph to data is on to start; unchecking asks for the axis through zero", async () => {
    render(SecurityDetailsModal, { security: 1, onclose: () => {} });
    const box = screen.getByRole("checkbox", { name: "Fit graph to data" }) as HTMLInputElement;
    expect(box.checked).toBe(true);
    await fireEvent.click(box);
    await waitFor(() => expect(chartCalls).toHaveBeenLastCalledWith(1, "market_value", "year", null, null, false));
  });

  it("the dropdown switches security; Edit saves name, ticker, and type", async () => {
    render(SecurityDetailsModal, { security: 1, onclose: () => {} });
    await fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    await fireEvent.input(screen.getByLabelText("Name"), { target: { value: "Total Market" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(update).toHaveBeenCalled());
    expect(update.mock.calls[0][0]).toBe(1);
    expect(update.mock.calls[0][1]).toMatchObject({ name: "Total Market", ticker: "VTSAX", security_type: "mutual_fund" });
    await fireEvent.change(screen.getByRole("combobox", { name: "Security" }), { target: { value: "2" } });
    await waitFor(() => expect(chartCalls).toHaveBeenLastCalledWith(2, "market_value", "year", null, null, true));
    expect(await screen.findByText("No transactions.")).toBeTruthy();
  });
});
