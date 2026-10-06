import { describe, expect, it } from "vitest";
import { MENUS, savedReportItems } from "./menus";

type Item = (typeof MENUS)[number]["items"][number];
const all = (xs: Item[]): Item[] => xs.flatMap((i) => [i, ...all(i.items ?? [])]);
const items = MENUS.flatMap((m) => all(m.items));

describe("menu definitions", () => {
  it("has the agreed menus in order", () => {
    expect(MENUS.map((m) => m.label)).toEqual(["File", "Edit", "Tools", "Reports", "Help"]);
    expect(MENUS[0].items.map((i) => i.label)).toEqual([
      "New…", "Open…", "Recent", "Rename Book…", "Back Up Now", "Restore…", "Import…", "Export…", "Integrity Check", "Exit",
    ]);
    expect(MENUS[2].items.map((i) => i.label)).toEqual([
      "Accounts", "Calendar", "Reminders", "Investments", "Insights", "Memorized Payees", "Categories", "Tags", "Securities", "Import Prices…", "Reconcile",
    ]);
    expect(MENUS[1].items.map((i) => i.label)).toEqual(["Undo (Ctrl+Z)", "Settings…", "Navigation Bar…", "Renaming…"]);
    expect(MENUS[3].items.map((i) => i.label)).toEqual(["Saved Reports", "Investing", "Net Worth", "Spending", "Comparison", "Tax"]);
    const sub = (label: string) => MENUS[3].items.find((i) => i.label === label)?.items?.map((i) => i.label);
    expect(sub("Saved Reports")).toEqual(["Manage Saved Reports…"]);
    expect(sub("Investing")).toEqual([
      "Capital Gains",
      "Investment Performance",
      "Investment Income",
      "Holdings",
      "Asset Allocation",
    ]);
    expect(sub("Net Worth")).toEqual(["Net Worth"]);
    expect(sub("Spending")).toEqual([
      "Itemized Categories",
      "Itemized Payees",
      "Income/Expense by Category",
      "Income/Expense by Payee",
    ]);
    expect(sub("Tax")).toEqual(["Capital Gains", "Tax Schedule", "Tax Summary"]);
  });

  it("Saved Reports lists each folder's reports by name, then Manage", () => {
    const manage = { id: "reports.saved", label: "Manage Saved Reports…" };
    const items = savedReportItems(
      [
        { id: 1, name: "Unfiled" },
        { id: 2, name: "titheable" },
        { id: 3, name: "Last Year" },
      ],
      [
        { id: 7, name: "Zoo", folder: 2 },
        { id: 8, name: "apples", folder: 2 },
        { id: 9, name: "Gains", folder: 1 },
      ],
      manage,
    );
    expect(items.map((i) => i.label)).toEqual(["Last Year", "titheable", "Unfiled", "Manage Saved Reports…"]);
    expect(items[0].items).toEqual([{ id: "reports.folder.3.empty", label: "(empty)", disabled: "No saved reports in this folder" }]);
    expect(items[1].items).toEqual([
      { id: "saved:8", label: "apples" },
      { id: "saved:7", label: "Zoo" },
    ]);
    expect(items[3]).toEqual({ ...manage, divider: true });
  });

  it("ids are unique and every greyed item says why", () => {
    const ids = items.map((i) => i.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const i of items.filter((x) => x.disabled !== undefined)) {
      expect(i.disabled, i.id).toMatch(/^Planned: \S/);
    }
  });

  it("what works today is not greyed", () => {
    const on = (id: string) => items.find((i) => i.id === id)?.disabled;
    for (const id of ["file.integrity", "file.exit", "edit.settings", "tools.accounts", "tools.calendar", "tools.reminders", "tools.investments", "tools.payees", "tools.categories", "tools.tags", "tools.securities", "tools.import_prices", "tools.download_prices", "edit.undo", "reports.saved", "reports.net_worth", "reports.tax_summary"]) {
      expect(on(id), id).toBeUndefined();
    }
  });
});
