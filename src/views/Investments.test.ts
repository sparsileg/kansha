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
      securityList: () => ok([{ id: 1, name: "Total Stock Market", ticker: "VTI", hidden: false }]),
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
  investViewState.portfolio = null;
});

describe("Investments view", () => {
  it("Download Prices, beside Customize, downloads for the As of date (PRC-040)", async () => {
    investViewState.asOf = "2026-06-12";
    render(Investments);
    await screen.findByText("Brokerage");
    const buttons = screen.getAllByRole("button").map((b) => b.textContent?.trim());
    expect(buttons.indexOf("Download Prices")).toBe(buttons.indexOf("Customize") + 1);
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
    const heads = screen.getAllByRole("columnheader").map((h) => h.textContent);
    expect(heads).toEqual([
      "Name", "Ticker Symbol", "Quote/Price", "Shares", "Market Value", "Gain/Loss",
      "Day Gain/Loss", "Price Day Change (%)",
    ]);
    expect(screen.getAllByText("10,050.00")).toHaveLength(2); // account row and Totals
    expect(screen.getByText("Totals:")).toBeTruthy();
    expect(screen.getByRole("combobox", { name: /View/ })).toBeTruthy();
    expect((screen.getByLabelText("As of") as HTMLInputElement).value).toBe("06/30/2026");
    expect(invPortfolio).toHaveBeenCalledWith([2], null, "2026-06-30");
  });

  it("expands an account, then an equity, to its lots", async () => {
    render(Investments);
    await screen.findByText("Brokerage");
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
    await waitFor(() => expect(invPortfolio).toHaveBeenLastCalledWith([2], null, "2026-07-04"));
    expect((screen.getByLabelText("As of") as HTMLInputElement).value).toBe("07/04/2026");
  });

  it("Customize: move Cost Basis in, rename, and the view follows", async () => {
    render(Investments);
    await screen.findByText("Brokerage");
    await fireEvent.click(screen.getByRole("button", { name: "Customize" }));
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
    await fireEvent.click(screen.getByRole("button", { name: "Customize" }));
    let dlg = screen.getByRole("dialog", { name: "Customize view" });
    await fireEvent.click(within(dlg).getByRole("button", { name: "Ticker Symbol" }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "<<Remove" }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "Cancel" }));
    expect(screen.getAllByRole("columnheader")).toHaveLength(8);
    await fireEvent.click(screen.getByRole("button", { name: "Customize" }));
    dlg = screen.getByRole("dialog", { name: "Customize view" });
    await fireEvent.click(within(dlg).getByRole("button", { name: "Ticker Symbol" }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "<<Remove" }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "Reset View" }));
    expect(within(dlg).getAllByRole("button", { name: "Ticker Symbol" })).toHaveLength(1);
    await fireEvent.click(within(dlg).getByRole("button", { name: "OK" }));
    expect(screen.getAllByRole("columnheader")).toHaveLength(8);
  });

  it("Accounts and Equities tabs hide what is unchecked", async () => {
    render(Investments);
    await screen.findByText("Brokerage");
    await fireEvent.click(screen.getByRole("button", { name: "Customize" }));
    const dlg = screen.getByRole("dialog", { name: "Customize view" });
    await fireEvent.click(within(dlg).getByRole("tab", { name: "Equities" }));
    await fireEvent.click(within(dlg).getByRole("checkbox", { name: /Total Stock Market/ }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(invPortfolio).toHaveBeenLastCalledWith([2], [], "2026-06-30"));
  });
});
