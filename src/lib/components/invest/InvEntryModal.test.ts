import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });
const invCreate = vi.fn((_: unknown) => ok(99));
const invUpdate = vi.fn((..._: unknown[]) => ok(null));
const stored = {
  account: 2, action: "dividend", date: "2026-03-15", settle_date: null, security: 1, quantity: null,
  price: null, commission: "0.00", amount: "12.00", split: null, to_account: null, lot_method: null,
  lots: [], acquired: null, counterpart: null, memo: "Q1", conversion: null,
};

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      invCreate: (input: unknown) => invCreate(input),
      invUpdate: (...a: unknown[]) => invUpdate(...a),
      invInput: () => ok(stored),
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
  invUpdate.mockClear();
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
    await fireEvent.click(screen.getByRole("button", { name: "Enter/Done" }));
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
    await fireEvent.click(screen.getByRole("button", { name: "Enter/Done" }));
    expect(screen.getByRole("alert").textContent).toMatch(/date/);
    expect(invCreate).not.toHaveBeenCalled();
  });

  it("a Roth conversion: only in an IRA or 401(k); in cash without a security, in kind with one", async () => {
    // A brokerage does not offer it.
    render(InvEntryModal, { account, txn: null, onclose: () => {} });
    const offered = () => [...(screen.getByLabelText("Action") as HTMLSelectElement).options].map((o) => o.value);
    expect(offered()).not.toContain("roth_conversion");
    cleanup();

    const ira = { id: 3, name: "IRA", status: "open", account_type: "traditional_ira", investment: {} } as never;
    const roth = { id: 4, name: "Roth", status: "open", account_type: "roth_ira", investment: {} } as never;
    listsState.accounts = [account, ira, roth] as never;
    render(InvEntryModal, { account: ira, txn: null, onclose: () => {} });
    expect(offered()).toContain("roth_conversion");
    await fireEvent.change(screen.getByLabelText("Action"), { target: { value: "roth_conversion" } });
    // In cash: no shares; only Roth IRAs to choose.
    expect(screen.queryByLabelText("Shares")).toBeNull();
    const to = screen.getByLabelText("To Roth IRA") as HTMLSelectElement;
    expect([...to.options].map((o) => o.textContent)).toEqual(["—", "Roth"]);
    await fireEvent.change(to, { target: { value: "4" } });
    await fireEvent.input(screen.getByLabelText("Value converted"), { target: { value: "10,000" } });
    await fireEvent.input(screen.getByLabelText(/Federal tax withheld/), { target: { value: "1,000" } });
    await fireEvent.click(screen.getByRole("button", { name: "Enter/Done" }));
    await waitFor(() => expect(invCreate).toHaveBeenCalledTimes(1));
    expect(invCreate.mock.calls[0][0]).toMatchObject({
      action: "roth_conversion",
      security: null,
      quantity: null,
      amount: "10000.00",
      to_account: 4,
      conversion: { nontaxable: "0.00", withheld_federal: "1000.00", withheld_state: "0.00" },
    });
    // In kind: a security brings shares and lots back.
    await fireEvent.change(screen.getByLabelText(/^Security/), { target: { value: "1" } });
    expect(screen.getByLabelText("Shares")).toBeTruthy();
    expect(screen.getByLabelText("Lots")).toBeTruthy();
  });

  it("Enter/Next saves, keeps Action and Trade date, clears the rest, and stays open", async () => {
    const onclose = vi.fn();
    const onentered = vi.fn();
    render(InvEntryModal, { account, txn: null, onclose, onentered });
    await fireEvent.change(screen.getByLabelText("Action"), { target: { value: "dividend" } });
    await fireEvent.input(screen.getByLabelText("Trade date"), { target: { value: "03/15/2026" } });
    await fireEvent.change(screen.getByLabelText(/Security/), { target: { value: "1" } });
    await fireEvent.input(screen.getByLabelText("Amount"), { target: { value: "12" } });
    await fireEvent.input(screen.getByLabelText("Memo"), { target: { value: "Q1" } });
    await fireEvent.click(screen.getByRole("button", { name: "Enter/Next" }));
    await waitFor(() => expect(invCreate).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(document.activeElement).toBe(screen.getByLabelText("Action")));
    expect(onentered).toHaveBeenCalledWith(true);
    expect(onclose).not.toHaveBeenCalled();
    expect((screen.getByLabelText("Action") as HTMLSelectElement).value).toBe("dividend");
    expect((screen.getByLabelText("Trade date") as HTMLInputElement).value).toBe("03/15/2026");
    expect((screen.getByLabelText("Amount") as HTMLInputElement).value).toBe("");
    expect((screen.getByLabelText("Memo") as HTMLInputElement).value).toBe("");
  });

  it("only the buttons save: Enter in a field does not", async () => {
    render(InvEntryModal, { account, txn: null, onclose: () => {} });
    await fireEvent.change(screen.getByLabelText("Action"), { target: { value: "dividend" } });
    await fireEvent.input(screen.getByLabelText("Amount"), { target: { value: "12" } });
    await fireEvent.submit(screen.getByLabelText("Amount").closest("form")!);
    expect(invCreate).not.toHaveBeenCalled();
  });

  it("Reset: a new transaction goes back to the defaults; Cancel closes unsaved", async () => {
    const onclose = vi.fn();
    render(InvEntryModal, { account, txn: null, onclose });
    await fireEvent.change(screen.getByLabelText("Action"), { target: { value: "dividend" } });
    await fireEvent.input(screen.getByLabelText("Trade date"), { target: { value: "03/15/2026" } });
    await fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect((screen.getByLabelText("Action") as HTMLSelectElement).value).toBe("buy");
    expect((screen.getByLabelText("Trade date") as HTMLInputElement).value).toBe("06/30/2026");
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onclose).toHaveBeenCalled();
    expect(invCreate).not.toHaveBeenCalled();
  });

  it("editing: Reset goes back to the stored values; Enter/Next saves the edit, then a new one", async () => {
    const onentered = vi.fn();
    render(InvEntryModal, { account, txn: 7, onclose: () => {}, onentered });
    const memo = () => screen.getByLabelText("Memo") as HTMLInputElement;
    await waitFor(() => expect(memo().value).toBe("Q1"));
    await fireEvent.input(memo(), { target: { value: "changed" } });
    await fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(memo().value).toBe("Q1");
    expect((screen.getByLabelText("Amount") as HTMLInputElement).value).toBe("12.00");

    await fireEvent.click(screen.getByRole("button", { name: "Enter/Next" }));
    await waitFor(() => expect(invUpdate).toHaveBeenCalledTimes(1));
    expect(invUpdate.mock.calls[0][0]).toBe(7);
    await waitFor(() => expect(screen.getByText(/New transaction/)).toBeTruthy());
    expect(onentered).toHaveBeenCalledWith(false);
    expect((screen.getByLabelText("Action") as HTMLSelectElement).value).toBe("dividend");
    expect(memo().value).toBe("");
    expect(screen.queryByRole("button", { name: "Delete" })).toBeNull();
    // Reset now means the defaults.
    await fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect((screen.getByLabelText("Action") as HTMLSelectElement).value).toBe("buy");
  });
});
