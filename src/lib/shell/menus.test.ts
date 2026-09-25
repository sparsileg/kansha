import { describe, expect, it } from "vitest";
import { MENUS } from "./menus";

const items = MENUS.flatMap((m) => m.items);

describe("menu definitions", () => {
  it("has the agreed menus in order", () => {
    expect(MENUS.map((m) => m.label)).toEqual(["File", "Edit", "Tools", "Reports", "Help"]);
    expect(MENUS[0].items.map((i) => i.label)).toEqual([
      "New…", "Open…", "Backup…", "Restore…", "Import…", "Export…", "Integrity Check", "Exit",
    ]);
    expect(MENUS[2].items.map((i) => i.label)).toEqual([
      "Accounts", "Calendar", "Reminders", "Payees", "Categories", "Tags", "Securities", "Reconcile",
    ]);
    expect(MENUS[3].items.map((i) => i.label)).toEqual(["Saved", "Investing", "Balances", "Spending", "Taxes"]);
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
    for (const id of ["file.integrity", "file.exit", "edit.settings", "tools.accounts", "tools.calendar", "tools.reminders", "tools.payees", "tools.categories", "tools.tags", "tools.securities"]) {
      expect(on(id), id).toBeUndefined();
    }
  });
});
