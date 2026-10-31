import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
let occurrences: unknown[] = [];
let posted: unknown[] = [];
const occCalls = vi.hoisted(() => vi.fn());
const txnCalls = vi.hoisted(() => vi.fn());
const skip = vi.hoisted(() => vi.fn());
const enterNow = vi.hoisted(() => vi.fn());
const prefill = vi.hoisted(() => vi.fn());

vi.mock("../lib/api", async (orig) => {
  const real = await orig<typeof import("../lib/api")>();
  return {
    ...real,
    commands: {
      calendarOccurrences: (...a: unknown[]) => (occCalls(...a), ok(occurrences)),
      calendarTransactions: (...a: unknown[]) => (txnCalls(...a), ok(posted)),
      scheduleSkip: (...a: unknown[]) => (skip(...a), ok(null)),
      scheduleEnter: (...a: unknown[]) => (enterNow(...a), ok({ schedule: 1, nominal: "2026-09-24", txn: 5, date: "2026-09-24" })),
      schedulePrefill: (...a: unknown[]) => (
        prefill(...a), ok({ account: 2, date: "2026-09-24", amount: "-10.00", lines: [], tags: [], payee: null, memo: "", notes: "", check_num: "", cleared: "unmarked" })
      ),
      registerQuery: () => ok({ rows: [], total: 0, today: "2026-09-24" }),
      registerSummary: () => ok({ current: "0.00", cleared: "0.00", ending: "0.00", available_credit: null }),
      calendarProjection: () => ok([]),
      scheduleList: () => ok([]),
      scheduleDueList: () => ok([]),
      scheduleReviewList: () => ok([]),
      accountBalances: () => ok([]),
      accountBar: () => ok({ balances: [], section_totals: [], net_worth: "0.00" }),
    },
  };
});

import { addMonths, displayDate, monthLabel, monthStart } from "../lib/format/date";
import { calendarState } from "../lib/state/calendar";
import { dialogState } from "../lib/state/dialogs.svelte";
import { registerState } from "../lib/state/register.svelte";
import { viewState } from "../lib/state/view.svelte";
import { listsState } from "../lib/state/lists.svelte";
import { scheduleState } from "../lib/state/schedule.svelte";
import Calendar from "./Calendar.svelte";

const item = (schedule: number, over: Record<string, unknown> = {}) => ({
  schedule,
  nominal: "2026-09-24",
  date: "2026-09-24",
  amount: "-10.00",
  status: "pending",
  account: 2,
  payee: schedule,
  estimated: false,
  mode: "remind",
  overridden: false,
  txn: null,
  needs_review: false,
  overdue: false,
  actionable: false,
  ...over,
});

beforeEach(() => {
  calendarState.month = null;
  registerState.prefill = null;
  viewState.reset();
  cleanup();
  vi.clearAllMocks();
  dialogState.schedule = undefined;
  listsState.today = "2026-09-24";
  listsState.payees = [1, 2, 3, 4, 5].map((id) => ({
    id, name: `Payee ${id}`, default_category: null, default_tag: null,
    default_memo: "", default_amount: null, hidden: false, created_at: "",
  }));
  occurrences = [1, 2, 3, 4, 5].map((n) => item(n));
  posted = [];
});

describe("Calendar bar", () => {
  it("lists only open spending accounts: checking, savings, cash, credit cards (CAL-040)", async () => {
    const acct = (id: number, name: string, account_type: string, status = "open") => ({ id, name, account_type, status });
    listsState.accounts = [
      acct(1, "Checking", "checking"),
      acct(2, "Savings", "savings"),
      acct(3, "Wallet", "cash"),
      acct(4, "Visa", "credit_card"),
      acct(5, "MM", "money_market"),
      acct(6, "Brokerage", "brokerage"),
      acct(7, "Mortgage", "loan"),
      acct(8, "Old Card", "credit_card", "closed"),
    ] as never;
    render(Calendar);
    const select = screen.getByRole("combobox", { name: "Calendar account" });
    const names = within(select).getAllByRole("option").map((o) => o.textContent);
    expect(names).toEqual(["All accounts", "Checking", "Savings", "Wallet", "Visa"]);
    const box = screen.getByRole("checkbox", { name: "Projected balance" }) as HTMLInputElement;
    expect(box.disabled).toBe(true);
    await fireEvent.change(select, { target: { value: "4" } });
    await waitFor(() => expect(box.disabled).toBe(false));
  });
});

describe("Calendar day dialog: Enter and Edit (REC-110)", () => {
  async function open(over: Record<string, unknown> = {}) {
    occurrences = [item(1, { actionable: true, ...over })];
    render(Calendar);
    const cell = await screen.findByRole("gridcell", { name: displayDate("2026-09-24") });
    await fireEvent.click(await within(cell).findByText(/Payee 1/));
    return screen.findByRole("dialog");
  }

  it("Enter records a fixed amount and stays on the Calendar", async () => {
    const dialog = await open();
    await fireEvent.click(within(dialog).getByRole("button", { name: "Enter" }));
    await waitFor(() =>
      expect(enterNow).toHaveBeenCalledWith(1, "2026-09-24", { date: null, amount: null, entry: null }, null, false),
    );
    expect(prefill).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).toBeTruthy();
    expect(viewState.current).toBe("insights");
  });

  it("Enter on an estimate opens the register prefilled", async () => {
    const dialog = await open({ estimated: true });
    await fireEvent.click(within(dialog).getByRole("button", { name: "Enter" }));
    await waitFor(() => expect(prefill).toHaveBeenCalledWith(1, "2026-09-24"));
    expect(enterNow).not.toHaveBeenCalled();
  });

  it("Edit on the next occurrence opens the register prefilled", async () => {
    const dialog = await open();
    await fireEvent.click(within(dialog).getByRole("button", { name: "Edit" }));
    await waitFor(() => expect(prefill).toHaveBeenCalledWith(1, "2026-09-24"));
    await waitFor(() => expect(registerState.prefill).toMatchObject({ schedule: 1, due: "2026-09-24" }));
    expect(enterNow).not.toHaveBeenCalled();
  });

  it("a double click opens it in the register and records nothing", async () => {
    const dialog = await open();
    await fireEvent.dblClick(within(dialog).getByRole("option"));
    await waitFor(() => expect(prefill).toHaveBeenCalled());
    expect(enterNow).not.toHaveBeenCalled();
  });

  it("Edit on a later occurrence opens its schedule", async () => {
    const edit = vi.spyOn(dialogState, "editSchedule").mockImplementation(() => {});
    scheduleState.rows = [{ schedule: { id: 1, fields: {} }, loan: null }] as never;
    const dialog = await open({ actionable: false });
    await fireEvent.click(within(dialog).getByRole("button", { name: "Edit" }));
    await waitFor(() => expect(edit).toHaveBeenCalled());
    expect(prefill).not.toHaveBeenCalled();
    edit.mockRestore();
    scheduleState.rows = [];
  });
});

describe("Calendar month (CAL-010)", () => {
  it("keeps the month last shown; Today returns to this month", async () => {
    const first = render(Calendar);
    await fireEvent.click(screen.getByRole("button", { name: "Next month" }));
    const label = monthLabel(addMonths(monthStart(listsState.today), 1));
    await waitFor(() => expect(screen.getByText(label)).toBeTruthy());
    first.unmount();
    render(Calendar);
    expect(screen.getByText(label)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Today" }));
    await waitFor(() => expect(screen.getByText(monthLabel(monthStart(listsState.today)))).toBeTruthy());
  });
});

describe("Calendar day with more than three items", () => {
  it("shows three chips and +2 more; selecting the day shows all five", async () => {
    render(Calendar);
    const cell = await screen.findByRole("gridcell", { name: displayDate("2026-09-24") });
    await waitFor(() => expect(within(cell).getAllByText(/Payee \d/)).toHaveLength(3));
    const more = within(cell).getByRole("button", { name: /\+2 more/ });
    await fireEvent.click(more);
    await waitFor(() => expect(within(cell).getAllByText(/Payee \d/)).toHaveLength(5));
    expect(within(cell).queryByRole("button", { name: /more/ })).toBeNull();
    // It only shows them: no dialog opens, and there is no day panel.
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByLabelText("Day")).toBeNull();
  });

  it("clicking a transaction opens its day's dialog with it chosen", async () => {
    occurrences = [item(1, { overdue: true, actionable: true }), item(2), item(3, { status: "entered", txn: 9 })];
    render(Calendar);
    const cell = await screen.findByRole("gridcell", { name: displayDate("2026-09-24") });
    await waitFor(() => expect(within(cell).getAllByText(/Payee \d/)).toHaveLength(3));
    await fireEvent.click(within(cell).getByText(/Payee 2/));
    const dialog = await screen.findByRole("dialog", { name: `Transactions: ${displayDate("2026-09-24")}` });
    // Open and done ones alike, whatever the calendar's checkbox says.
    expect(occCalls).toHaveBeenLastCalledWith("2026-09-24", "2026-09-24", null, true);
    const options = await within(dialog).findAllByRole("option");
    expect(options).toHaveLength(3);
    expect(options[1].getAttribute("aria-selected")).toBe("true");
    expect(options[0].textContent).toContain("Overdue");
    expect(options[2].textContent).toContain("Entered");
    const names = within(dialog)
      .getAllByRole("button")
      .map((b) => b.textContent?.trim())
      .filter((n) => n !== "×"); // the dialog's own close box
    expect(names).toEqual(["New schedule", "Enter", "Edit", "Skip", "Close"]);
    // Payee 2 is not its schedule's next one: Enter and Skip are off.
    expect((within(dialog).getByRole("button", { name: "Enter" }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(options[0]);
    expect((within(dialog).getByRole("button", { name: "Enter" }) as HTMLButtonElement).disabled).toBe(false);
    await fireEvent.click(within(dialog).getByRole("button", { name: "Skip" }));
    await waitFor(() => expect(skip).toHaveBeenCalledWith(1, "2026-09-24"));
    await fireEvent.click(within(dialog).getAllByRole("button", { name: "Close" }).at(-1)!);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("clicking a day's blank space opens its dialog; New Schedule starts on that day", async () => {
    occurrences = [item(1), item(2)];
    render(Calendar);
    const cell = await screen.findByRole("gridcell", { name: displayDate("2026-09-24") });
    await fireEvent.click(cell);
    const dialog = await screen.findByRole("dialog");
    const options = await within(dialog).findAllByRole("option");
    expect(options[0].getAttribute("aria-selected")).toBe("true");
    await fireEvent.click(within(dialog).getByRole("button", { name: "New schedule" }));
    expect(dialogState.schedule).toEqual({ id: null, fields: null, start: "2026-09-24" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // The keyboard opens it too.
    await fireEvent.keyDown(cell, { key: "Enter" });
    expect(await screen.findByRole("dialog")).toBeTruthy();
  });

  it("clicking another day while the dialog is open updates it in place", async () => {
    occurrences = [item(1)];
    render(Calendar);
    await fireEvent.click(await screen.findByRole("gridcell", { name: displayDate("2026-09-24") }));
    const first = await screen.findByRole("dialog", { name: `Transactions: ${displayDate("2026-09-24")}` });
    // Modeless: the calendar behind stays clickable (jsdom ignores the CSS).
    expect(first.getAttribute("aria-modal")).toBe("false");
    occCalls.mockClear();
    await fireEvent.click(screen.getByRole("gridcell", { name: displayDate("2026-09-23") }));
    await waitFor(() =>
      expect(screen.getByRole("dialog", { name: `Transactions: ${displayDate("2026-09-23")}` })).toBeTruthy(),
    );
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    expect(occCalls).toHaveBeenCalledWith("2026-09-23", "2026-09-23", null, true);
  });

  it("register transactions show with Show entered transactions, and always in the day's dialog", async () => {
    occurrences = [item(1)];
    posted = [{ txn: 40, date: "2026-09-24", account: 2, payee: 4, amount: "-12.00" }];
    render(Calendar);
    const cell = await screen.findByRole("gridcell", { name: displayDate("2026-09-24") });
    await waitFor(() => expect(within(cell).getAllByText(/Payee \d/)).toHaveLength(1));
    expect(txnCalls).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByLabelText("Show entered transactions"));
    await waitFor(() => expect(within(cell).getAllByText(/Payee \d/)).toHaveLength(2));
    expect(txnCalls).toHaveBeenLastCalledWith(expect.any(String), expect.any(String), null);

    await fireEvent.click(within(cell).getByText(/Payee 4/));
    const dialog = await screen.findByRole("dialog");
    expect(txnCalls).toHaveBeenLastCalledWith("2026-09-24", "2026-09-24", null);
    const options = await within(dialog).findAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual([expect.stringContaining("Due"), expect.stringContaining("Register")]);
    expect(options[1].getAttribute("aria-selected")).toBe("true");
    // Not scheduled: it can be opened, not entered or skipped.
    const enterBtn = within(dialog).getByRole("button", { name: "Enter" }) as HTMLButtonElement;
    expect(enterBtn.disabled).toBe(true);
    expect(enterBtn.title).toBe("Not a scheduled transaction");
    expect((within(dialog).getByRole("button", { name: "Edit" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("strikes through skipped items only, not entered or register ones", async () => {
    occurrences = [item(1, { status: "skipped" }), item(2, { status: "entered", txn: 9 })];
    posted = [{ txn: 40, date: "2026-09-24", account: 2, payee: 4, amount: "-12.00" }];
    render(Calendar);
    await fireEvent.click(screen.getByLabelText("Show entered transactions"));
    const cell = await screen.findByRole("gridcell", { name: displayDate("2026-09-24") });
    await waitFor(() => expect(within(cell).getAllByText(/Payee \d/)).toHaveLength(3));
    const chip = (n: number) => within(cell).getByText(new RegExp(`Payee ${n}`)).closest(".chip")!;
    expect(chip(1).classList.contains("skipped")).toBe(true);
    expect(chip(2).classList.contains("skipped")).toBe(false);
    expect(chip(4).classList.contains("skipped")).toBe(false);
    await fireEvent.click(within(cell).getByText(/Payee 4/));
    const options = await within(await screen.findByRole("dialog")).findAllByRole("option");
    expect(options.map((o) => o.classList.contains("skipped"))).toEqual([true, false, false]);
  });

  it("marks an overdue chip with the word, not only a color", async () => {
    occurrences = [item(1, { overdue: true })];
    render(Calendar);
    expect(await screen.findByText("Overdue")).toBeTruthy();
  });
});
