import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/svelte";

vi.mock("../lib/api", async (orig) => {
  const real = await orig<typeof import("../lib/api")>();
  return { ...real, commands: {} };
});

import Reconcile from "./Reconcile.svelte";
import { dateFormatState } from "../lib/state/dateformat.svelte";
import { selectOnFocus } from "../lib/ui/selectOnFocus";
import { listsState } from "../lib/state/lists.svelte";
import { reconcileState } from "../lib/state/reconcile.svelte";

const item = (id: number, amount: string, checked = false) => ({
  txn_id: id, date: "2026-01-05", check_num: "", payee_name: "Costco", memo: "", amount, checked,
});
const session = (difference: string, over: Record<string, unknown> = {}) =>
  ({
    reconciliation: { id: 5, account: 1, statement_date: "2026-01-31", statement_balance: "900.00", status: "in_progress" },
    payments: [item(10, "-100.00", true)],
    deposits: [item(11, "500.00")],
    opening: "0.00",
    checked_payments: "-100.00",
    checked_payment_count: 1,
    checked_deposits: "0.00",
    checked_deposit_count: 0,
    cleared_balance: "-100.00",
    difference,
    opening_check: { expected: "0.00", actual: "0.00", changed: [], matches: true },
    ...over,
  }) as never;

beforeEach(() => {
  cleanup();
  listsState.today = "2026-02-10";
  listsState.accounts = [
    { id: 1, name: "Checking", status: "open", account_type: "checking" },
    { id: 2, name: "Brokerage", status: "open", account_type: "brokerage" },
  ] as never;
  listsState.categories = [];
  reconcileState.accountId = 1;
  reconcileState.session = null;
  reconcileState.opening = null;
  reconcileState.history = [];
  reconcileState.error = null;
});

describe("Reconcile view", () => {
  it("offers only accounts that can be reconciled", () => {
    render(Reconcile);
    const options = within(screen.getByLabelText("Account")).getAllByRole("option").map((o) => o.textContent);
    expect(options).toEqual(["Checking"]);
  });

  it("with no session, shows the statement form and the history", () => {
    reconcileState.opening = { expected: "1400.00", actual: "1400.00", changed: [], matches: true };
    reconcileState.history = [
      {
        reconciliation: { id: 4, account: 1, statement_date: "2025-12-31", statement_balance: "1400.00", opening_balance: "0.00", status: "finished" },
        item_count: 3,
        items_total: "1400.00",
      },
    ] as never;
    render(Reconcile);
    expect(screen.getByRole("button", { name: "Start reconciling" })).toBeTruthy();
    expect(screen.getByText("Opening balance: 1,400.00")).toBeTruthy();
    expect(screen.getByText("12/31/2025")).toBeTruthy();
    expect(screen.getByText("Finished")).toBeTruthy();
  });

  it("Finish waits for a zero difference", () => {
    reconcileState.session = session("500.00");
    render(Reconcile);
    expect((screen.getByRole("button", { name: "Finish" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: /Balance Adjustment/ }) as HTMLButtonElement).disabled).toBe(false);
    expect(screen.getByText("≠ Not yet zero")).toBeTruthy();
  });

  it("a zero difference is ready to finish and has nothing to adjust", () => {
    reconcileState.session = session("0.00");
    render(Reconcile);
    expect((screen.getByRole("button", { name: "Finish" }) as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByRole("button", { name: /Balance Adjustment/ }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText("✓ Ready to finish")).toBeTruthy();
  });

  it("lists items with their check state", () => {
    reconcileState.session = session("500.00");
    render(Reconcile);
    const boxes = screen.getAllByRole("checkbox") as HTMLInputElement[];
    // Two header check-alls, then one box per item.
    expect(boxes.map((b) => b.checked)).toEqual([true, true, false, false]);
  });

  it("focusing a field selects what is in it, to type over", () => {
    // App.svelte installs this for the whole app.
    const stop = selectOnFocus(document);
    reconcileState.session = session("500.00");
    render(Reconcile);
    const balance = screen.getByLabelText("Ending balance") as HTMLInputElement;
    balance.focus();
    expect(balance.value).toBe("900.00");
    expect([balance.selectionStart, balance.selectionEnd]).toEqual([0, balance.value.length]);
    stop();
  });

  it("shows dates in the chosen format", () => {
    reconcileState.session = session("500.00");
    dateFormatState.set("ymd");
    try {
      render(Reconcile);
      expect((screen.getByLabelText("Statement date") as HTMLInputElement).value).toBe("2026-01-31");
      expect(screen.getAllByText("2026-01-05").length).toBe(2);
    } finally {
      dateFormatState.set("mdy");
    }
  });

  it("puts each checkbox by the amount", () => {
    reconcileState.session = session("500.00");
    render(Reconcile);
    const row = screen.getByRole("checkbox", { name: /Check Costco/, checked: false }).closest("tr")!;
    const cells = [...row.querySelectorAll("td")];
    expect(cells.at(-2)!.textContent).toBe("500.00");
    expect(cells.at(-1)!.querySelector("input[type=checkbox]")).toBeTruthy();
  });

  it("a click anywhere on a row toggles it", async () => {
    reconcileState.session = session("500.00");
    const check = vi.spyOn(reconcileState, "check").mockResolvedValue(true as never);
    render(Reconcile);
    const unchecked = screen.getByRole("checkbox", { name: /Check Costco/, checked: false });
    (within(unchecked.closest("tr")!).getByText("500.00") as HTMLElement).click();
    expect(check).toHaveBeenCalledWith([11], true);
    check.mockClear();
    const checked = screen.getAllByRole("checkbox", { name: /Check Costco/, checked: true })[0];
    (within(checked.closest("tr")!).getByText("Costco") as HTMLElement).click();
    expect(check).toHaveBeenCalledWith([10], false);
    check.mockClear();
    // The checkbox itself toggles once, not twice.
    unchecked.click();
    expect(check).toHaveBeenCalledTimes(1);
    expect(check).toHaveBeenCalledWith([11], true);
    check.mockRestore();
  });

  it("warns when reconciled transactions changed since the last statement", () => {
    reconcileState.session = session("0.00", {
      opening_check: {
        expected: "1400.00",
        actual: "1380.00",
        matches: false,
        changed: [{ txn_id: 9, date: "2026-01-05", was: "-100.00", now: "-120.00", action: "update" }],
      },
    });
    render(Reconcile);
    expect(screen.getByText(/Opening balance changed/)).toBeTruthy();
    expect(screen.getByText(/01\/05\/2026: was -100.00, now/)).toBeTruthy();
  });
});
