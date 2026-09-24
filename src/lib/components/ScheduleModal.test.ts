import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";

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

  it("offers the frequency choices and extra fields for Nth weekday", async () => {
    render(ScheduleModal, { id: null, fields: null, start: "2026-10-01" });
    const freq = screen.getByLabelText("Frequency") as HTMLSelectElement;
    expect(Array.from(freq.options).map((o) => o.textContent)).toContain("Quarterly");
    await fireEvent.change(freq, { target: { value: "nth_weekday" } });
    await waitFor(() => expect(screen.getByLabelText("Weekday")).toBeTruthy());
    expect(screen.getByLabelText("Week")).toBeTruthy();
  });
});
