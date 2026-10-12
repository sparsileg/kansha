import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";

vi.mock("../lib/api", async (orig) => {
  const real = await orig<typeof import("../lib/api")>();
  return { ...real, commands: {} };
});

import Scheduled from "./Scheduled.svelte";
import { listsState } from "../lib/state/lists.svelte";
import { scheduleState } from "../lib/state/schedule.svelte";

const row = (id: number, account: number, direction: "payment" | "deposit") =>
  ({
    schedule: {
      id,
      status: "active",
      fields: { account, direction, payee: null, remind_days: 7, end: { kind: "never" }, mode: "remind", amount_type: "fixed" },
    },
    due_date: "2026-11-01",
    amount: "-10.00",
    how_often: "Monthly",
    left: null,
  }) as never;

describe("Scheduled", () => {
  it("names Method for each schedule's account (REC-300)", () => {
    const acct = (id: number, name: string, account_type: string, investment: unknown = null) =>
      ({ id, name, status: "open", account_type, investment }) as never;
    listsState.accounts = [
      acct(1, "Checking", "checking"),
      acct(2, "Visa", "credit_card"),
      acct(3, "Mortgage", "loan"),
      acct(4, "Brokerage", "brokerage", { cash_mode: "internal", linked_cash_account: null }),
      acct(5, "House", "other_asset"),
    ];
    scheduleState.rows = [
      row(1, 1, "payment"),
      row(2, 1, "deposit"),
      row(3, 2, "payment"),
      row(4, 2, "deposit"),
      row(5, 3, "deposit"),
      row(6, 4, "payment"),
      row(7, 3, "payment"),
      row(8, 5, "deposit"),
    ];
    render(Scheduled);
    const methods = screen.getAllByRole("row").slice(1).map((r) => r.querySelectorAll("td")[4].textContent);
    expect(methods).toEqual(["Payment", "Deposit", "Charge", "Payment", "Decrease", "Cash out", "Increase", "Increase"]);
  });
});
