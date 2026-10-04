import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";

const skip = vi.fn();
const prefill = vi.fn();
const enterNow = vi.fn();
const override = vi.fn();
vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
  return {
    ...real,
    commands: {
      schedulePrefill: (...a: unknown[]) => {
        prefill(...a);
        return ok({ account: 2, date: "2026-10-01", amount: "-1000.00", lines: [], tags: [], payee: null, memo: "", notes: "", check_num: "", cleared: "unmarked" });
      },
      scheduleSkip: (...a: unknown[]) => {
        skip(...a);
        return ok(null);
      },
      scheduleOverride: (...a: unknown[]) => {
        override(...a);
        return ok(null);
      },
      scheduleEnter: (...a: unknown[]) => enterNow(...a),
      scheduleList: () => ok([]),
      scheduleDueList: () => ok([]),
      scheduleReviewList: () => ok([]),
      accountBalances: () => ok([]),
      registerQuery: () => ok({ rows: [], total: 0, today: "2026-10-01" }),
      registerSummary: () => ok({ current: "0.00", cleared: "0.00", ending: "0.00", available_credit: null }),
    },
  };
});

import OccurrenceRow from "./OccurrenceRow.svelte";
import { confirmState } from "../state/confirm.svelte";
import { listsState } from "../state/lists.svelte";
import { scheduleState } from "../state/schedule.svelte";
import { registerState } from "../state/register.svelte";
import { viewState } from "../state/view.svelte";
import type { OccurrenceView } from "../types/bindings";

const view = (over: Partial<OccurrenceView> = {}): OccurrenceView => ({
  schedule: 1,
  nominal: "2026-10-01",
  date: "2026-10-01",
  amount: "-1000.00",
  direction: "payment",
  status: "pending",
  account: 2,
  payee: null,
  estimated: false,
  mode: "remind",
  overridden: false,
  txn: null,
  needs_review: false,
  overdue: false,
  actionable: true,
  ...over,
});

beforeEach(() => {
  vi.clearAllMocks();
  registerState.prefill = null;
  viewState.navigate("insights");
});

describe("OccurrenceRow", () => {
  it("Enter opens the account register with the occurrence prefilled", async () => {
    render(OccurrenceRow, { view: view() });
    await fireEvent.click(screen.getByRole("button", { name: "Enter" }));
    await waitFor(() => expect(registerState.prefill).not.toBeNull());
    expect(prefill).toHaveBeenCalledWith(1, "2026-10-01");
    expect(viewState.current).toBe("account");
    expect(registerState.accountId).toBe(2);
    expect(registerState.prefill).toMatchObject({ schedule: 1, due: "2026-10-01" });
  });

  it("an estimate is entered the same way (the register shows the amount)", async () => {
    render(OccurrenceRow, { view: view({ estimated: true }) });
    await fireEvent.click(screen.getByRole("button", { name: "Enter" }));
    await waitFor(() => expect(prefill).toHaveBeenCalled());
  });

  it("a later occurrence offers no Enter or Skip", () => {
    render(OccurrenceRow, { view: view({ actionable: false }) });
    expect(screen.queryByRole("button", { name: "Enter" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Skip" })).toBeNull();
    expect(screen.getByRole("button", { name: "Edit…" })).toBeTruthy();
  });

  it("a cash out of an investment account is entered as scheduled, after asking; a failure shows", async () => {
    listsState.accounts = [{ id: 2, name: "Brokerage", status: "open", investment: { cash_mode: "internal" } }] as never;
    scheduleState.rows = [
      { schedule: { id: 1, fields: { account: 2, payee: null, lines: [{ target: { kind: "account", id: 3 } }] } } },
    ] as never;
    const ask = vi.spyOn(confirmState, "ask").mockResolvedValue(true);
    enterNow.mockResolvedValueOnce({ status: "ok", data: {} });
    render(OccurrenceRow, { view: view() });
    await fireEvent.click(screen.getByRole("button", { name: "Enter" }));
    await waitFor(() =>
      expect(enterNow).toHaveBeenCalledWith(1, "2026-10-01", { date: null, amount: null, entry: null }, null, true),
    );
    expect(ask).toHaveBeenCalled();
    expect(prefill).not.toHaveBeenCalled();

    // Entering reloads the list (empty here): put the schedule back.
    scheduleState.rows = [
      { schedule: { id: 1, fields: { account: 2, payee: null, lines: [{ target: { kind: "account", id: 3 } }] } } },
    ] as never;
    enterNow.mockResolvedValueOnce({ status: "error", error: { kind: "invalid", message: "account is closed" } });
    await fireEvent.click(screen.getByRole("button", { name: "Enter" }));
    expect(await screen.findByText(/account is closed/)).toBeTruthy();
    scheduleState.rows = [];
    listsState.accounts = [];
  });

  it("an amount typed for a 0.00 payment goes out, not in", async () => {
    render(OccurrenceRow, { view: view({ amount: "0.00", direction: "payment" }) });
    await fireEvent.click(screen.getByRole("button", { name: "Edit…" }));
    await fireEvent.input(screen.getByLabelText("Amount"), { target: { value: "125" } });
    await fireEvent.click(screen.getByRole("button", { name: "Set for this occurrence only" }));
    await waitFor(() => expect(override).toHaveBeenCalledWith(1, "2026-10-01", null, "-125.00"));
  });

  it("Skip calls skip", async () => {
    render(OccurrenceRow, { view: view() });
    await fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    await waitFor(() => expect(skip).toHaveBeenCalledWith(1, "2026-10-01"));
  });
});
