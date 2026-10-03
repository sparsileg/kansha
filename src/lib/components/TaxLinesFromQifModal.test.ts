import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";

const { pickImportFile, taxLinesFromQifPreview, taxLinesFromQifApply } = vi.hoisted(() => {
  const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });
  let applied = false;
  const plan = () => ({
    items: [
      { qif_name: "Salary Spouse", code: 8096, category: 4, tax_line: null, status: "unmapped" },
      { qif_name: "Tax:Real Estate", code: 4416, category: 2, tax_line: 24, status: applied ? "kept" : "set" },
      { qif_name: "Interest Inc", code: 4592, category: 3, tax_line: 1, status: "kept" },
    ],
  });
  return {
    pickImportFile: vi.fn((_start: string | null) => Promise.resolve("/q/all.qif" as string | null)),
    taxLinesFromQifPreview: vi.fn((_path: string) => ok(plan())),
    taxLinesFromQifApply: vi.fn((_path: string) => {
      applied = true;
      return ok(1);
    }),
  };
});

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return { ...real, commands: { pickImportFile, taxLinesFromQifPreview, taxLinesFromQifApply } };
});

import TaxLinesFromQifModal from "./TaxLinesFromQifModal.svelte";
import { listsState } from "../state/lists.svelte";

beforeEach(() => {
  cleanup();
  listsState.categories = [
    { id: 1, parent: null, kind: "expense", name: "Tax", tax_related: false, tax_line: null, hidden: false, system: null },
    { id: 2, parent: 1, kind: "expense", name: "Real Estate", tax_related: false, tax_line: null, hidden: false, system: null },
    { id: 3, parent: null, kind: "income", name: "Interest Inc", tax_related: true, tax_line: 1, hidden: false, system: null },
    { id: 4, parent: null, kind: "income", name: "Salary Spouse", tax_related: true, tax_line: null, hidden: false, system: null },
  ] as never;
  listsState.taxLines = [
    { id: 1, form: "Form 1040", line: "Other income, misc.", sort_order: 200 },
    { id: 24, form: "Schedule A", line: "Real estate taxes", sort_order: 650 },
  ];
  vi.spyOn(listsState, "loadAll").mockResolvedValue();
});

describe("TaxLinesFromQifModal", () => {
  it("previews the file, lists 'set' first, applies, and reloads", async () => {
    render(TaxLinesFromQifModal, { onclose: () => {} });
    const apply = () => screen.getByRole("button", { name: "Apply" }) as HTMLButtonElement;
    expect(apply().disabled).toBe(true);
    await fireEvent.click(screen.getByRole("button", { name: "Choose QIF file…" }));
    expect(await screen.findByText("1 of 3 coded categories get a tax line.")).toBeTruthy();
    expect(taxLinesFromQifPreview).toHaveBeenCalledWith("/q/all.qif");

    const rows = screen.getAllByRole("row").slice(1).map((r) => r.textContent);
    expect(rows[0]).toContain("Set");
    expect(rows[0]).toContain("Tax:Real Estate");
    expect(rows[0]).toContain("Schedule A: Real estate taxes");
    expect(rows[2]).toContain("No Kansha line");

    expect(apply().disabled).toBe(false);
    await fireEvent.click(apply());
    expect(await screen.findByText(/Tax lines set on 1 category\./)).toBeTruthy();
    expect(taxLinesFromQifApply).toHaveBeenCalledWith("/q/all.qif");
    expect(listsState.loadAll).toHaveBeenCalled();
    await vi.waitFor(() => expect(apply().disabled).toBe(true));
  });
});
