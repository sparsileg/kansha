import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });
const invExportCsv = vi.fn((..._: unknown[]) => ok("/home/u/Downloads/Brokerage transactions 2026-06-30.csv"));
const reportRange = vi.fn((r: { preset: string; from: string | null; to: string | null }) =>
  ok(r.preset === "custom" ? { from: r.from, to: r.to ?? "2026-06-30" } : { from: "2026-01-01", to: "2026-06-30" }),
);

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      invExportCsv: (...a: unknown[]) => invExportCsv(...a),
      reportRange: (r: never) => reportRange(r),
    },
  };
});

import InvExportModal from "./InvExportModal.svelte";
import { listsState } from "../../state/lists.svelte";
import { statusState } from "../../state/status.svelte";

const account = { id: 2, name: "Brokerage", status: "open", investment: {} } as never;

beforeEach(() => {
  cleanup();
  invExportCsv.mockClear();
  listsState.today = "2026-06-30";
});

describe("Export CSV… on the investment register (INV-080)", () => {
  it("Year to date first; Export sends the range, says where, and closes", async () => {
    const show = vi.spyOn(statusState, "show");
    const onclose = vi.fn();
    render(InvExportModal, { account, onclose });
    expect((screen.getByLabelText("Date range") as HTMLSelectElement).value).toBe("year_to_date");
    await waitFor(() => expect(screen.getByText(/From 01\/01\/2026/)).toBeTruthy());
    await fireEvent.click(screen.getByRole("button", { name: "Export" }));
    await waitFor(() => expect(onclose).toHaveBeenCalled());
    expect(invExportCsv).toHaveBeenCalledWith(2, { preset: "year_to_date", from: null, to: null });
    expect(show).toHaveBeenCalledWith(expect.stringMatching(/^Saved to .*Brokerage transactions/));
  });

  it("Custom dates start from the dates shown and are sent as chosen", async () => {
    render(InvExportModal, { account, onclose: () => {} });
    await waitFor(() => expect(screen.getByText(/From 01\/01\/2026/)).toBeTruthy());
    await fireEvent.change(screen.getByLabelText("Date range"), { target: { value: "custom" } });
    expect(screen.getByLabelText("From")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Export" }));
    await waitFor(() => expect(invExportCsv).toHaveBeenCalledTimes(1));
    expect(invExportCsv.mock.calls[0][1]).toEqual({ preset: "custom", from: "2026-01-01", to: "2026-06-30" });
  });

  it("Cancel closes without writing", async () => {
    const onclose = vi.fn();
    render(InvExportModal, { account, onclose });
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onclose).toHaveBeenCalled();
    expect(invExportCsv).not.toHaveBeenCalled();
  });
});
