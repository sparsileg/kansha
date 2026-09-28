import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
let occurrences: unknown[] = [];
const occCalls = vi.hoisted(() => vi.fn());
const skip = vi.hoisted(() => vi.fn());

vi.mock("../lib/api", async (orig) => {
  const real = await orig<typeof import("../lib/api")>();
  return {
    ...real,
    commands: {
      calendarOccurrences: (...a: unknown[]) => (occCalls(...a), ok(occurrences)),
      scheduleSkip: (...a: unknown[]) => (skip(...a), ok(null)),
      calendarProjection: () => ok([]),
      scheduleList: () => ok([]),
      scheduleDueList: () => ok([]),
      scheduleReviewList: () => ok([]),
      accountBalances: () => ok([]),
    },
  };
});

import { displayDate } from "../lib/format/date";
import { dialogState } from "../lib/state/dialogs.svelte";
import { listsState } from "../lib/state/lists.svelte";
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
  cleanup();
  vi.clearAllMocks();
  dialogState.schedule = undefined;
  listsState.today = "2026-09-24";
  listsState.payees = [1, 2, 3, 4, 5].map((id) => ({
    id, name: `Payee ${id}`, default_category: null, default_tag: null,
    default_memo: "", default_amount: null, hidden: false, created_at: "",
  }));
  occurrences = [1, 2, 3, 4, 5].map((n) => item(n));
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
    // The day panel lists every item too, one line each.
    const panel = screen.getByLabelText("Day");
    expect(within(panel).getAllByRole("button", { name: /Payee \d/ })).toHaveLength(5);
    expect(within(panel).queryByRole("button", { name: "Skip" })).toBeNull();
  });

  it("clicking a transaction opens its day's dialog with it chosen", async () => {
    occurrences = [item(1, { overdue: true, actionable: true }), item(2), item(3, { status: "entered", txn: 9 })];
    render(Calendar);
    const cell = await screen.findByRole("gridcell", { name: displayDate("2026-09-24") });
    await waitFor(() => expect(within(cell).getAllByText(/Payee \d/)).toHaveLength(3));
    await fireEvent.click(within(cell).getByText(/Payee 2/));
    const dialog = await screen.findByRole("dialog", { name: `Scheduled transactions: ${displayDate("2026-09-24")}` });
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
    expect(names).toEqual(["Enter", "Edit", "Skip", "Close", "New Schedule"]);
    // Payee 2 is not its schedule's next one: Enter and Skip are off.
    expect((within(dialog).getByRole("button", { name: "Enter" }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(options[0]);
    expect((within(dialog).getByRole("button", { name: "Enter" }) as HTMLButtonElement).disabled).toBe(false);
    await fireEvent.click(within(dialog).getByRole("button", { name: "Skip" }));
    await waitFor(() => expect(skip).toHaveBeenCalledWith(1, "2026-09-24"));
    await fireEvent.click(within(dialog).getAllByRole("button", { name: "Close" }).at(-1)!);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("double-clicking a day's blank space opens its dialog; New Schedule starts on that day", async () => {
    occurrences = [item(1), item(2)];
    render(Calendar);
    const cell = await screen.findByRole("gridcell", { name: displayDate("2026-09-24") });
    await fireEvent.dblClick(cell);
    const dialog = await screen.findByRole("dialog");
    const options = await within(dialog).findAllByRole("option");
    expect(options[0].getAttribute("aria-selected")).toBe("true");
    await fireEvent.click(within(dialog).getByRole("button", { name: "New Schedule" }));
    expect(dialogState.schedule).toEqual({ id: null, fields: null, start: "2026-09-24" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("marks an overdue chip with the word, not only a color", async () => {
    occurrences = [item(1, { overdue: true })];
    render(Calendar);
    expect(await screen.findByText("Overdue")).toBeTruthy();
  });
});
