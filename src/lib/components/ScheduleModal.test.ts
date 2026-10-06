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

  it("the amount is fixed or the average of past payments (REC-065)", async () => {
    render(ScheduleModal, { id: null, fields: null, start: "2026-10-01" });
    expect(screen.queryByLabelText("Amount is an estimate (confirm each time)")).toBeNull();
    const kind = screen.getByLabelText("Amount") as HTMLSelectElement;
    expect(Array.from(kind.options).map((o) => o.textContent)).toEqual(["Fixed", "Average of past payments"]);
    expect(screen.queryByLabelText("Payments to average")).toBeNull();
    await fireEvent.change(kind, { target: { value: "average" } });
    const count = (await screen.findByLabelText("Payments to average")) as HTMLInputElement;
    expect(count.value).toBe("3");
    const amount = screen.getByLabelText("Line 1 amount") as HTMLInputElement;
    expect(amount.readOnly).toBe(true);
    expect(amount.placeholder).toBe("Average");
    // An average is entered by the user, on one line.
    const auto = within(screen.getByLabelText("Mode")).getByRole("option", { name: /automatically/ }) as HTMLOptionElement;
    expect(auto.disabled).toBe(true);
    expect((screen.getByRole("button", { name: "Split" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("has one Scheduling section and no weekend choice", () => {
    render(ScheduleModal, { id: null, fields: null, start: "2026-10-01" });
    const legends = screen.getAllByRole("group").map((g) => g.querySelector("legend")?.textContent);
    expect(legends).toContain("Scheduling");
    for (const gone of ["How often", "Ends", "Entering"]) expect(legends).not.toContain(gone);
    expect(screen.queryByLabelText("On a weekend")).toBeNull();
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
