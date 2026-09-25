import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

const register = {
  account: 2,
  rows: [
    {
      txn_id: 10, date: "2026-01-10", settle_date: null, action: "buy", action_label: "Buy",
      security: 1, security_label: "VTI", quantity: "10", price: "200", commission: "5.00",
      split: null, amount: "-2005.00", cash_balance: "7995.00", memo: "", cleared: "cleared",
      other_account: null, other_category: null, incoming: false, future: false,
    },
    {
      txn_id: 11, date: "2026-02-01", settle_date: null, action: "transfer_shares", action_label: "Transfer In",
      security: 1, security_label: "VTI", quantity: "5", price: null, commission: "0.00",
      split: null, amount: "0.00", cash_balance: "7995.00", memo: "", cleared: null,
      other_account: 3, other_category: null, incoming: true, future: false,
    },
  ],
  cash: "7995.00",
  negative_cash: false,
  today: "2026-06-30",
};
const holdings = {
  account: 2, as_of: "2026-06-30", cash: "7995.00", basis: "2005.00", market_value: "3200.00",
  total_value: "11195.00", unrealized: "1195.00", missing_prices: false, stale_prices: true,
  positions: [
    {
      account: 2, security: 1, name: "Total Stock Market", ticker: "VTI", security_type: "etf",
      asset_class: "us_equity", shares: "10", basis: "2005.00", price: "320", price_date: "2026-06-01",
      stale: true, market_value: "3200.00", unrealized: "1195.00",
    },
  ],
};

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      securityList: () => ok([{ id: 1, name: "Total Stock Market", ticker: "VTI", hidden: false }]),
      invRegister: () => ok(register),
      invHoldings: () => ok(holdings),
      invLots: () => ok([]),
      invIncome: () => ok({ rows: [], total: { security: null, security_label: "", dividends: "0.00", interest: "0.00", cg_short: "0.00", cg_long: "0.00", other: "0.00", total: "0.00" } }),
      invPerformance: () => ok({ account: 2, as_of: "2026-06-30", rows: [], total: { security: null, security_label: "Total", basis: "2005.00", market_value: "3200.00", unrealized: "1195.00", realized: "0.00", income: "0.00", total_gain: "1195.00", total_return: "59.60" } }),
      invGains: () => ok([]),
      invAllocation: () => ok({ as_of: "2026-06-30", accounts: [2], rows: [], total: "0.00", missing_prices: false }),
      invInput: () => new Promise(() => {}),
      invTradeAmount: () => ok("2005.00"),
    },
  };
});

import InvestmentAccount from "./InvestmentAccount.svelte";
import { investState } from "../../state/invest.svelte";
import { listsState } from "../../state/lists.svelte";

const account = {
  id: 2, name: "Brokerage", status: "open", account_type: "brokerage", tax_treatment: "taxable",
  investment: { cash_mode: "internal", linked_cash_account: null, mmf_mode: "cash", default_lot_method: "fifo", subtype: null },
} as never;

beforeEach(() => {
  cleanup();
  listsState.today = "2026-06-30";
  listsState.accounts = [account, { id: 3, name: "IRA", status: "open", account_type: "traditional_ira", investment: {} }] as never;
  investState.accountId = null;
  investState.tab = "transactions";
});

describe("Investment account view (POS-040)", () => {
  it("has the six tabs and lists the register with its cash balance", async () => {
    render(InvestmentAccount, { account });
    for (const t of ["Overview", "Transactions", "Holdings", "Lots", "Income", "Performance"]) {
      expect(screen.getByRole("tab", { name: t })).toBeTruthy();
    }
    await waitFor(() => expect(screen.getByText("-2,005.00")).toBeTruthy());
    expect(screen.getAllByText("7,995.00").length).toBeGreaterThan(0);
    expect(screen.getByText("Transfer In")).toBeTruthy();
    expect(screen.getByText(/from IRA/)).toBeTruthy();
  });

  it("holdings flag a stale price in words, not only color", async () => {
    render(InvestmentAccount, { account });
    await fireEvent.click(screen.getByRole("tab", { name: "Holdings" }));
    await waitFor(() => expect(screen.getByText("3,200.00", { selector: "td" })).toBeTruthy());
    expect(screen.getByText(/stale/)).toBeTruthy();
    expect(screen.getByText("320.00")).toBeTruthy();
  });

  it("opens the entry dialog for a new transaction", async () => {
    render(InvestmentAccount, { account });
    await fireEvent.click(screen.getByRole("button", { name: "New transaction…" }));
    expect(screen.getByRole("dialog", { name: /New transaction/ })).toBeTruthy();
  });
});
