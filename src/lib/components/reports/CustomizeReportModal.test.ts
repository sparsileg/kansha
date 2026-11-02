import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      reportColumns: () => Promise.resolve([]),
      reportRange: () => ok({ from: "2025-01-01", to: "2025-12-31" }),
      reportPeriodChoices: () => ok([]),
    },
  };
});

import CustomizeReportModal from "./CustomizeReportModal.svelte";
import { listsState } from "../../state/lists.svelte";
import type { ReportSettings } from "../../types/bindings";

const itemized = (over: Partial<ReportSettings> = {}): ReportSettings =>
  ({
    kind: "itemized_categories",
    title: "Itemized Categories",
    range: { preset: "last_year", from: null, to: null },
    subtotal: "term",
    tax_group: "category",
    compare: "previous_period",
    compare_group: "category",
    interval: "none",
    sort: "date",
    sort_desc: false,
    hidden_columns: [],
    cents: true,
    totals_only: false,
    totals_on_heading: null,
    show_zero: false,
    transfers: false,
    by_tag: false,
    flat: false,
    cleared: null,
    amount_min: null,
    amount_max: null,
    cash_flow: false,
    budget_lines: false,
    accounts: null,
    categories: null,
    payees: null,
    securities: null,
    tags: null,
    ...over,
  }) as ReportSettings;

describe("Customize an Itemized report (RPT-200)", () => {
  listsState.today = "2026-06-30";

  it("Subtotal by None, cleared statuses, and an amount range are saved", async () => {
    const onsave = vi.fn();
    render(CustomizeReportModal, { settings: itemized(), onsave, onclose: () => {} });
    await fireEvent.change(screen.getByRole("combobox", { name: "Subtotal by" }), { target: { value: "none" } });
    await fireEvent.click(screen.getByRole("checkbox", { name: "Reconciled" }));
    await fireEvent.input(screen.getByRole("textbox", { name: "Amount from" }), { target: { value: "$40" } });
    await fireEvent.input(screen.getByRole("textbox", { name: "to" }), { target: { value: "-1,000" } });
    await fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() => expect(onsave).toHaveBeenCalled());
    expect(onsave.mock.calls[0][0]).toMatchObject({
      flat: true,
      by_tag: false,
      cleared: ["unmarked", "cleared"],
      amount_min: "40.00",
      amount_max: "1000.00",
    });
  });

  it("all statuses ticked and blank amounts are no filter; a bad amount is refused", async () => {
    const onsave = vi.fn();
    render(CustomizeReportModal, {
      settings: itemized({ cleared: ["cleared"], amount_min: "5.00" }),
      onsave,
      onclose: () => {},
    });
    expect((screen.getByRole("textbox", { name: "Amount from" }) as HTMLInputElement).value).toBe("5.00");
    await fireEvent.click(screen.getByRole("checkbox", { name: "Uncleared" }));
    await fireEvent.click(screen.getByRole("checkbox", { name: "Reconciled" }));
    await fireEvent.input(screen.getByRole("textbox", { name: "to" }), { target: { value: "abc" } });
    await fireEvent.click(screen.getByRole("button", { name: "OK" }));
    expect(await screen.findByText(/Enter amounts like 100.00/)).toBeTruthy();
    expect(onsave).not.toHaveBeenCalled();
    await fireEvent.input(screen.getByRole("textbox", { name: "to" }), { target: { value: "" } });
    await fireEvent.input(screen.getByRole("textbox", { name: "Amount from" }), { target: { value: "" } });
    await fireEvent.click(screen.getByRole("button", { name: "OK" }));
    await waitFor(() => expect(onsave).toHaveBeenCalled());
    expect(onsave.mock.calls[0][0]).toMatchObject({ cleared: null, amount_min: null, amount_max: null });
  });
});
