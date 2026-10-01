import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";

const create = vi.fn();
vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
  return {
    ...real,
    commands: {
      scheduleCreate: (f: unknown) => {
        create(f);
        return ok({});
      },
      scheduleList: () => ok([]),
      scheduleDueList: () => ok([]),
      scheduleReviewList: () => ok([]),
      accountBalances: () => ok([]),
    },
  };
});

import ScheduleModal from "./ScheduleModal.svelte";
import { listsState } from "../state/lists.svelte";

describe("ScheduleModal", () => {
  it("shows a validation message and does not save an incomplete form", async () => {
    listsState.today = "2026-09-24";
    render(ScheduleModal, { id: null, fields: null, start: "2026-10-01" });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect((await screen.findByRole("alert")).textContent).toContain("Choose an account");
    expect(create).not.toHaveBeenCalled();
  });

  it("lists investment accounts, except ones whose cash is linked", async () => {
    listsState.today = "2026-09-24";
    const acct = (id: number, name: string, investment: unknown) =>
      ({ id, name, status: "open", investment }) as never;
    const inv = (cash_mode: "internal" | "linked") => ({ cash_mode, linked_cash_account: null });
    listsState.accounts = [
      acct(1, "Checking", null),
      acct(2, "Brokerage 448", inv("internal")),
      acct(3, "Linked IRA", inv("linked")),
    ];
    render(ScheduleModal, { id: null, fields: null, start: "2026-10-01" });
    const names = within(screen.getByLabelText("Account")).getAllByRole("option").map((o) => o.textContent);
    expect(names).toEqual(["(choose)", "Checking", "Brokerage 448"]);
  });

  it("offers the frequency choices and extra fields for Nth weekday", async () => {
    render(ScheduleModal, { id: null, fields: null, start: "2026-10-01" });
    const freq = screen.getByLabelText("Frequency") as HTMLSelectElement;
    expect(Array.from(freq.options).map((o) => o.textContent)).toContain("Quarterly");
    await fireEvent.change(freq, { target: { value: "nth_weekday" } });
    await waitFor(() => expect(screen.getByLabelText("Weekday")).toBeTruthy());
    expect(screen.getByLabelText("Week")).toBeTruthy();
  });

  it("a split line's amount keeps one leading '-' (paycheck deduction)", async () => {
    render(ScheduleModal, { id: null, fields: null, start: "2026-10-01" });
    await fireEvent.click(screen.getByRole("button", { name: "Split" }));
    const amount = screen.getByLabelText("Line 2 amount") as HTMLInputElement;
    await fireEvent.input(amount, { target: { value: "-5-00" } });
    expect(amount.value).toBe("-500");
  });
});
