import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });
const invCreate = vi.fn((_: unknown) => ok(99));

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      invCreate: (input: unknown) => invCreate(input),
      invTradeAmount: () => ok("2005.00"),
      invLots: () =>
        ok([
          { id: 41, acquired: "2025-01-10", open_quantity: "10", open_basis: "1000.00", per_share: "100", term: "long", security: 1 },
        ]),
      securityList: () => ok([]),
      invRegister: () => ok({ account: 2, rows: [], cash: "0.00", negative_cash: false, today: "2026-06-30" }),
      invHoldings: () => ok(null),
      invIncome: () => ok(null),
      invPerformance: () => ok(null),
      invGains: () => ok([]),
      invAllocation: () => ok(null),
      accountBalances: () => ok([]),
    },
  };
});

import InvEntryModal from "./InvEntryModal.svelte";
import { investState } from "../../state/invest.svelte";
import { listsState } from "../../state/lists.svelte";

const account = { id: 2, name: "Brokerage", status: "open", investment: {} } as never;

beforeEach(() => {
  cleanup();
  invCreate.mockClear();
  listsState.today = "2026-06-30";
  listsState.accounts = [account] as never;
  listsState.categories = [];
  investState.securities = [{ id: 1, name: "Total Stock Market", ticker: "VTI", hidden: false }] as never;
  investState.accountId = null;
});

describe("Investment entry dialog (INV-030)", () => {
  it("shows the fields a buy takes and the amount from Rust", async () => {
    render(InvEntryModal, { account, txn: null, onclose: () => {} });
    expect(screen.getByLabelText("Shares")).toBeTruthy();
    expect(screen.getByLabelText("Commission")).toBeTruthy();
    await fireEvent.input(screen.getByLabelText("Shares"), { target: { value: "10" } });
    await fireEvent.input(screen.getByLabelText("Price per share"), { target: { value: "200" } });
    await fireEvent.input(screen.getByLabelText("Commission"), { target: { value: "5" } });
    await waitFor(() => expect(screen.getByText(/= 2,005.00/)).toBeTruthy());
  });

  it("a dividend has no shares, price, or commission", async () => {
    render(InvEntryModal, { account, txn: null, onclose: () => {} });
    await fireEvent.change(screen.getByLabelText("Action"), { target: { value: "dividend" } });
    expect(screen.queryByLabelText("Shares")).toBeNull();
    expect(screen.queryByLabelText("Commission")).toBeNull();
    expect(screen.getByLabelText("Amount")).toBeTruthy();
  });

  it("choosing lots lists the open lots; save sends the input", async () => {
    const onclose = vi.fn();
    render(InvEntryModal, { account, txn: null, onclose });
    await fireEvent.change(screen.getByLabelText("Action"), { target: { value: "sell" } });
    await fireEvent.change(screen.getByLabelText("Security"), { target: { value: "1" } });
    await fireEvent.input(screen.getByLabelText("Shares"), { target: { value: "4" } });
    await fireEvent.input(screen.getByLabelText(/Net proceeds/), { target: { value: "500" } });
    await fireEvent.change(screen.getByLabelText("Lots"), { target: { value: "specific" } });
    const pick = await screen.findByLabelText("Shares from lot acquired 01/10/2025");
    await fireEvent.input(pick, { target: { value: "4" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(invCreate).toHaveBeenCalledTimes(1));
    expect(invCreate.mock.calls[0][0]).toMatchObject({
      action: "sell",
      security: 1,
      quantity: "4",
      amount: "500.00",
      lot_method: "specific",
      lots: [{ lot: 41, quantity: "4" }],
    });
    await waitFor(() => expect(onclose).toHaveBeenCalled());
  });

  it("a date that does not parse is caught before anything is sent", async () => {
    render(InvEntryModal, { account, txn: null, onclose: () => {} });
    await fireEvent.input(screen.getByLabelText("Trade date"), { target: { value: "99/99" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(screen.getByRole("alert").textContent).toMatch(/date/);
    expect(invCreate).not.toHaveBeenCalled();
  });
});
