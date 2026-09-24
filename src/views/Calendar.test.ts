import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
let occurrences: unknown[] = [];

vi.mock("../lib/api", async (orig) => {
  const real = await orig<typeof import("../lib/api")>();
  return {
    ...real,
    commands: {
      calendarOccurrences: () => ok(occurrences),
      calendarProjection: () => ok([]),
      scheduleList: () => ok([]),
      scheduleDueList: () => ok([]),
      scheduleReviewList: () => ok([]),
      accountBalances: () => ok([]),
    },
  };
});

import { displayDate } from "../lib/format/date";
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

  it("clicking an item opens its details, flags and buttons in a modal", async () => {
    occurrences = [item(1, { overdue: true, actionable: true }), item(2)];
    render(Calendar);
    const cell = await screen.findByRole("gridcell", { name: displayDate("2026-09-24") });
    await fireEvent.click(cell);
    const panel = screen.getByLabelText("Day");
    await fireEvent.click(within(panel).getByRole("button", { name: /Payee 1/ }));
    const dialog = await screen.findByRole("dialog", { name: "Scheduled transaction" });
    expect(within(dialog).getByText("Overdue")).toBeTruthy();
    for (const name of ["Enter", "Skip", "Edit…"]) {
      expect(within(dialog).getByRole("button", { name })).toBeTruthy();
    }
    expect(within(dialog).queryByText(/Payee 2/)).toBeNull(); // only this one
    await fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("marks an overdue chip with the word, not only a color", async () => {
    occurrences = [item(1, { overdue: true })];
    render(Calendar);
    expect(await screen.findByText("Overdue")).toBeTruthy();
  });
});
