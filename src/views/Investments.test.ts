import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

const totals = {
  basis: "3100.00", market_value: "10050.00", gain: "50.00", day_gain: "150.00",
  day_percent: "5.00", missing_prices: false, stale_prices: false,
};
const portfolio = {
  as_of: "2026-06-30",
  accounts: [
    {
      account: 2, cash: "6900.00", totals,
      positions: [
        {
          security: 1, name: "Total Stock Market", ticker: "VTI", shares: "15", basis: "3100.00",
          price: "210", price_date: "2026-06-30", stale: false, market_value: "3150.00", gain: "50.00",
          day_gain: "150.00", day_percent: "5.00",
          lots: [{ lot: 7, acquired: "2026-02-01", shares: "15", basis: "3100.00", market_value: "3150.00", gain: "50.00", day_gain: "150.00" }],
          sales: [],
        },
      ],
    },
  ],
  total: totals,
};
const invPortfolio = vi.fn((..._args: unknown[]) => ok(portfolio));
const pricesDownload = vi.fn((..._args: unknown[]) => ok({ stored: 1, failed: [] }));

vi.mock("../lib/api", async (orig) => {
  const real = await orig<typeof import("../lib/api")>();
  return {
    ...real,
    commands: {
      invPortfolio: (...a: unknown[]) => invPortfolio(...a),
      pricesDownload: (...a: unknown[]) => pricesDownload(...a),
      invAccounts: () => ok([]),
      accountBalances: () => ok([]),
      securityList: () => ok([{ id: 1, name: "Total Stock Market", ticker: "VTI", hidden: false, security_type: "etf" }]),
      securityTransactions: () => ok([]),
      securityChart: () => ok({ dates: [], labels: [], series: [], ticks: [], zero: 0, x_unit: "month" }),
    },
  };
});

import Investments from "./Investments.svelte";
import { investViewState } from "../lib/state/investview.svelte";
import { listsState } from "../lib/state/lists.svelte";
import { defaultViews } from "../lib/invest/views";

beforeEach(() => {
  cleanup();
  invPortfolio.mockClear();
  listsState.today = "2026-06-30";
  listsState.accounts = [
    { id: 2, name: "Brokerage", status: "open", account_type: "brokerage", investment: {} },
  ] as never;
  const d = defaultViews();
  investViewState.views = d.views;
  investViewState.selected = 0;
  investViewState.asOf = "";
  investViewState.expanded = new Set();
  investViewState.seenAccounts = new Set();
  investViewState.portfolio = null;
});

/** Customize, from the gear's menu. */
async function customize() {
  await fireEvent.click(screen.getByRole("button", { name: "Investments options" }));
  await fireEvent.click(screen.getByRole("menuitem", { name: "Customize…" }));
}

describe("Investments view", () => {
  it("Download Prices downloads for the As of date (PRC-040); Customize is in the gear's menu", async () => {
    investViewState.asOf = "2026-06-12";
    render(Investments);
    await screen.findByText("Brokerage");
    expect(screen.queryByRole("button", { name: "Customize" })).toBeNull();
    invPortfolio.mockClear();
    await fireEvent.click(screen.getByRole("button", { name: "Download Prices" }));
    await waitFor(() => expect(pricesDownload).toHaveBeenCalledWith("2026-06-12"));
    await waitFor(() => expect(invPortfolio).toHaveBeenCalled());
  });

  it("with no date chosen, downloads for today", async () => {
    render(Investments);
    await screen.findByText("Brokerage");
    await fireEvent.click(screen.getByRole("button", { name: "Download Prices" }));
    await waitFor(() => expect(pricesDownload).toHaveBeenLastCalledWith("2026-06-30"));
  });

  it("shows the Default view: 8 columns, collapsed account with rolled-up values, Totals", async () => {
    render(Investments);
    await waitFor(() => expect(screen.getByText("Brokerage")).toBeTruthy());
    await fireEvent.click(screen.getByRole("button", { name: "Collapse Brokerage" }));
    const heads = screen.getAllByRole("columnheader").map((h) => h.textContent);
    expect(heads).toEqual([
      "Name", "Ticker Symbol", "Quote/Price", "Shares", "Market Value", "Gain/Loss",
      "Day Gain/Loss", "Price Day Change (%)",
    ]);
    expect(screen.getAllByText("10,050.00")).toHaveLength(2); // account row and Totals
    expect(screen.getByText("Totals:")).toBeTruthy();
    expect(screen.getByRole("combobox", { name: /View/ })).toBeTruthy();
    expect((screen.getByLabelText("As of") as HTMLInputElement).value).toBe("06/30/2026");
    expect(invPortfolio).toHaveBeenCalledWith([2], null, "2026-06-30", false);
  });

  it("an account starts expanded once; a collapse is kept while the app runs", async () => {
    render(Investments);
    await screen.findByText("Total Stock Market");
    await fireEvent.click(screen.getByRole("button", { name: "Collapse Brokerage" }));
    expect(screen.queryByText("Total Stock Market")).toBeNull();
    cleanup();
    invPortfolio.mockClear();
    render(Investments);
    await waitFor(() => expect(invPortfolio).toHaveBeenCalled());
    await screen.findByRole("button", { name: "Expand Brokerage" });
    expect(screen.queryByText("Total Stock Market")).toBeNull();
  });

  it("expands an account, then an equity, to its lots", async () => {
    render(Investments);
    await screen.findByText("Brokerage");
    await fireEvent.click(screen.getByRole("button", { name: "Collapse Brokerage" }));
    await fireEvent.click(screen.getByRole("button", { name: "Expand Brokerage" }));
    expect(screen.getByText("Cash")).toBeTruthy();
    expect(screen.getByText("Total Stock Market")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Expand Total Stock Market" }));
    expect(screen.getByText("Lot 02/01/2026")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Collapse Total Stock Market" }));
    expect(screen.queryByText("Lot 02/01/2026")).toBeNull();
  });

  it("the calendar's double arrows move one month; a day sets the date", async () => {
    render(Investments);
    await screen.findByText("Brokerage");
    await fireEvent.click(screen.getByRole("button", { name: "Show calendar" }));
    expect(screen.getByText("June 2026")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Previous month" }));
    expect(screen.getByText("May 2026")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Next month" }));
    await fireEvent.click(screen.getByRole("button", { name: "Next month" }));
    expect(screen.getByText("July 2026")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "07/04/2026" }));
    await waitFor(() => expect(invPortfolio).toHaveBeenLastCalledWith([2], null, "2026-07-04", false));
    expect((screen.getByLabelText("As of") as HTMLInputElement).value).toBe("07/04/2026");
  });

  it("Customize: move Cost Basis in, rename, and the view follows", async () => {
    render(Investments);
    await screen.findByText("Brokerage");
    await customize();
    const dlg = screen.getByRole("dialog", { name: "Customize view" });
    await fireEvent.input(within(dlg).getByLabelText("Name of view"), { target: { value: "Mine" } });
    await fireEvent.click(within(dlg).getByRole("button", { name: "Cost Basis" }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "Add>>" }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "Move Up" }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const heads = screen.getAllByRole("columnheader").map((h) => h.textContent);
    expect(heads.at(-2)).toBe("Cost Basis");
    expect(heads.at(-1)).toBe("Price Day Change (%)");
    expect(screen.getByRole("option", { name: "Mine" })).toBeTruthy();
  });

  it("Reset View restores the defaults; Cancel changes nothing", async () => {
    render(Investments);
    await screen.findByText("Brokerage");
    await customize();
    let dlg = screen.getByRole("dialog", { name: "Customize view" });
    await fireEvent.click(within(dlg).getByRole("button", { name: "Ticker Symbol" }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "<<Remove" }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "Cancel" }));
    expect(screen.getAllByRole("columnheader")).toHaveLength(8);
    await customize();
    dlg = screen.getByRole("dialog", { name: "Customize view" });
    await fireEvent.click(within(dlg).getByRole("button", { name: "Ticker Symbol" }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "<<Remove" }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "Reset View" }));
    expect(within(dlg).getAllByRole("button", { name: "Ticker Symbol" })).toHaveLength(1);
    await fireEvent.click(within(dlg).getByRole("button", { name: "OK" }));
    expect(screen.getAllByRole("columnheader")).toHaveLength(8);
  });

  it("Accounts and Securities tabs hide what is unchecked", async () => {
    render(Investments);
    await screen.findByText("Brokerage");
    await customize();
    const dlg = screen.getByRole("dialog", { name: "Customize view" });
    await fireEvent.click(within(dlg).getByRole("tab", { name: "Securities" }));
    await fireEvent.click(within(dlg).getByRole("checkbox", { name: /Total Stock Market/ }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(invPortfolio).toHaveBeenLastCalledWith([2], [], "2026-06-30", false));
  });

  it("the gear's Show closed lots asks Rust for sales, and shows it is on", async () => {
    render(Investments);
    await screen.findByText("Brokerage");
    await fireEvent.click(screen.getByRole("button", { name: "Investments options" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Show closed lots" }));
    await waitFor(() => expect(invPortfolio).toHaveBeenLastCalledWith([2], null, "2026-06-30", true));
    await fireEvent.click(screen.getByRole("button", { name: "Investments options" }));
    expect(screen.getByRole("menuitem", { name: "✓ Show closed lots" })).toBeTruthy();
    await fireEvent.click(screen.getByRole("menuitem", { name: "✓ Show closed lots" }));
    await waitFor(() => expect(invPortfolio).toHaveBeenLastCalledWith([2], null, "2026-06-30", false));
  });

  it("Show closed lots is kept per view: another view starts without it", async () => {
    render(Investments);
    await screen.findByText("Brokerage");
    await fireEvent.click(screen.getByRole("button", { name: "Investments options" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Show closed lots" }));
    await waitFor(() => expect(invPortfolio).toHaveBeenLastCalledWith([2], null, "2026-06-30", true));
    expect(investViewState.views[0].showClosed).toBe(true);
    investViewState.select(1);
    await waitFor(() => expect(invPortfolio).toHaveBeenLastCalledWith([2], null, "2026-06-30", false));
    investViewState.select(0);
    await waitFor(() => expect(invPortfolio).toHaveBeenLastCalledWith([2], null, "2026-06-30", true));
  });

  it("clicking a security opens its details", async () => {
    render(Investments);
    await screen.findByText("Brokerage");
    await fireEvent.click(screen.getByRole("button", { name: "Total Stock Market" }));
    const dlg = await screen.findByRole("dialog", { name: "Security Details" });
    expect((within(dlg).getByRole("combobox", { name: "Security" }) as HTMLSelectElement).value).toBe("1");
  });
});
