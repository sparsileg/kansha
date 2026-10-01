import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const chartCalls = vi.hoisted(() => vi.fn());
const update = vi.hoisted(() => vi.fn());
const priceSet = vi.hoisted(() => vi.fn());
const priceDelete = vi.hoisted(() => vi.fn());
const asked = vi.hoisted(() => vi.fn());
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
      priceList: () =>
        ok([
          { security: 1, date: "2026-06-29", price: "102.5000", source: "manual" },
          { security: 1, date: "2026-06-01", price: "99.0000", source: "manual" },
        ]),
      priceSet: (...a: unknown[]) => (priceSet(...a), ok(null)),
      priceDelete: (...a: unknown[]) => (priceDelete(...a), ok(null)),
      securityUpdate: (...a: unknown[]) => (update(...a), ok(sec(1, "Total Market", "VTSAX"))),
    },
  };
});

import SecurityDetailsModal from "./SecurityDetailsModal.svelte";
import { investState } from "../../state/invest.svelte";
import { listsState } from "../../state/lists.svelte";
import { confirmState } from "../../state/confirm.svelte";
import { dateFormatState } from "../../state/dateformat.svelte";

beforeEach(async () => {
  chartCalls.mockClear();
  update.mockClear();
  dateFormatState.set("mdy");
  listsState.today = "2026-06-30";
  listsState.accounts = [{ id: 5, name: "Brokerage" }] as never;
  await investState.loadSecurities();
});

describe("Security Details: Update Prices", () => {
  const card = () => screen.getByRole("article", { name: "Update Prices" });

  it("lists prices newest first with New, Edit, and Delete", async () => {
    render(SecurityDetailsModal, { security: 1, onclose: () => {} });
    await waitFor(() => expect(within(card()).getByText("06/29/2026")).toBeTruthy());
    const rows = within(card()).getAllByRole("row").slice(1).map((r) => r.textContent);
    expect(rows).toEqual(["06/29/2026102.5000", "06/01/202699.0000"]);
    expect((within(card()).getByRole("button", { name: "Edit" }) as HTMLButtonElement).disabled).toBe(true);
    expect((within(card()).getByRole("button", { name: "Delete" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("New adds a top row dated today and saves the price entered", async () => {
    priceSet.mockClear();
    render(SecurityDetailsModal, { security: 1, onclose: () => {} });
    await waitFor(() => expect(within(card()).getByText("06/29/2026")).toBeTruthy());
    await fireEvent.click(within(card()).getByRole("button", { name: "New" }));
    const rows = within(card()).getAllByRole("row");
    expect((within(rows[1]).getByRole("textbox", { name: "Price date" }) as HTMLInputElement).value).toBe("06/30/2026");
    await fireEvent.input(within(card()).getByRole("textbox", { name: "Price" }), { target: { value: "103.25" } });
    await fireEvent.click(within(card()).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(priceSet).toHaveBeenCalledWith(1, "2026-06-30", "103.25"));
  });

  it("Edit changes the price of the selected row; its date is fixed", async () => {
    priceSet.mockClear();
    render(SecurityDetailsModal, { security: 1, onclose: () => {} });
    await waitFor(() => expect(within(card()).getByText("06/01/2026")).toBeTruthy());
    await fireEvent.click(within(card()).getByText("06/01/2026"));
    await fireEvent.click(within(card()).getByRole("button", { name: "Edit" }));
    expect(within(card()).queryByRole("textbox", { name: "Price date" })).toBeNull();
    await fireEvent.input(within(card()).getByRole("textbox", { name: "Price" }), { target: { value: "100" } });
    await fireEvent.click(within(card()).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(priceSet).toHaveBeenCalledWith(1, "2026-06-01", "100"));
  });

  it("Delete asks first, then removes the selected row's price", async () => {
    priceDelete.mockClear();
    asked.mockReset();
    const ask = vi.spyOn(confirmState, "ask").mockImplementation(async (m: string) => (asked(m), true));
    render(SecurityDetailsModal, { security: 1, onclose: () => {} });
    await waitFor(() => expect(within(card()).getByText("06/29/2026")).toBeTruthy());
    await fireEvent.click(within(card()).getByText("06/29/2026"));
    await fireEvent.click(within(card()).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(priceDelete).toHaveBeenCalledWith(1, "2026-06-29"));
    expect(asked).toHaveBeenCalledOnce();
    ask.mockRestore();
  });
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
    const info = screen.getByRole("article", { name: "Security" });
    await fireEvent.click(within(info).getByRole("button", { name: "Edit" }));
    await fireEvent.input(screen.getByLabelText("Name"), { target: { value: "Total Market" } });
    await fireEvent.click(within(info).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(update).toHaveBeenCalled());
    expect(update.mock.calls[0][0]).toBe(1);
    expect(update.mock.calls[0][1]).toMatchObject({ name: "Total Market", ticker: "VTSAX", security_type: "mutual_fund" });
    await fireEvent.change(screen.getByRole("combobox", { name: "Security" }), { target: { value: "2" } });
    await waitFor(() => expect(chartCalls).toHaveBeenLastCalledWith(2, "market_value", "year", null, null, true));
    expect(await screen.findByText("No transactions.")).toBeTruthy();
  });
});
