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
vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      securityList: () => ok([{ id: 1, name: "Total Stock Market", ticker: "VTI", hidden: false }]),
      invRegister: () => ok(register),
      invInput: () => new Promise(() => {}),
      invTradeAmount: () => ok("2005.00"),
    },
  };
});

import InvestmentAccount from "./InvestmentAccount.svelte";
import { investState } from "../../state/invest.svelte";
import { listsState } from "../../state/lists.svelte";
import { registerState } from "../../state/register.svelte";

const account = {
  id: 2, name: "Brokerage", status: "open", account_type: "brokerage", tax_treatment: "taxable",
  investment: { cash_mode: "internal", linked_cash_account: null, mmf_mode: "cash", default_lot_method: "fifo", subtype: null },
} as never;

beforeEach(() => {
  cleanup();
  listsState.today = "2026-06-30";
  listsState.accounts = [account, { id: 3, name: "IRA", status: "open", account_type: "traditional_ira", investment: {} }] as never;
  investState.accountId = null;
});

describe("Investment account register (INV-030)", () => {
  it("stripes rows; future rows are italic in their own tint; reconciled rows are gray", async () => {
    const saved = register.rows;
    const at = (txn_id: number, o: object) => ({ ...saved[0], txn_id, ...o });
    register.rows = [
      at(20, { cleared: "reconciled" }),
      at(21, {}),
      at(22, { date: "2026-07-01", future: true }),
      at(23, { date: "2026-07-02", future: true }),
    ];
    try {
      render(InvestmentAccount, { account });
      await waitFor(() => expect(screen.getAllByText("Buy")).toHaveLength(4));
      const cls = screen.getAllByText("Buy").map((td) => [...td.closest("tr")!.classList].filter((x) => !x.startsWith("svelte-")).sort().join(" "));
      expect(cls).toEqual(["reconciled", "alt", "future", "alt future"]);
    } finally {
      register.rows = saved;
    }
  });

  it("lists the register with its cash balance and has no tabs", async () => {
    render(InvestmentAccount, { account });
    await waitFor(() => expect(screen.getByText("-2,005.00")).toBeTruthy());
    expect(screen.getAllByText("7,995.00").length).toBeGreaterThan(0);
    expect(screen.getByText("Transfer In")).toBeTruthy();
    expect(screen.getByText(/from IRA/)).toBeTruthy();
    expect(screen.queryByRole("tab")).toBeNull();
    expect(screen.queryByRole("button", { name: "New transaction…" })).toBeNull();
  });

  it("opens the entry dialog when typing in the empty line", async () => {
    render(InvestmentAccount, { account });
    const blank = await screen.findByLabelText("New transaction");
    await fireEvent.keyDown(blank, { key: "Tab" });
    expect(screen.queryByRole("dialog")).toBeNull();
    await fireEvent.keyDown(blank, { key: "b" });
    expect(screen.getByRole("dialog", { name: /New transaction/ })).toBeTruthy();
  });

  it("has no empty line in a closed account", async () => {
    render(InvestmentAccount, { account: { ...(account as object), status: "closed" } as never });
    await waitFor(() => expect(screen.getByText("-2,005.00")).toBeTruthy());
    expect(screen.queryByLabelText("New transaction")).toBeNull();
  });

  it("selects and reveals the transaction a search hit names", async () => {
    registerState.accountId = 2;
    registerState.selected = 10;
    registerState.reveal = 10;
    try {
      const { container } = render(InvestmentAccount, { account });
      await waitFor(() => expect(container.querySelector('tr[data-txn="10"]')?.classList.contains("sel")).toBe(true));
      expect(container.querySelector('tr[data-txn="11"]')?.classList.contains("sel")).toBe(false);
      await waitFor(() => expect(registerState.reveal).toBeNull());
    } finally {
      registerState.accountId = null;
      registerState.selected = null;
      registerState.reveal = null;
    }
  });
});
