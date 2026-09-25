import { describe, expect, it } from "vitest";
import { DEFAULT_NAV, addNav, moveNav, navCatalog, parseNav, removeNav, resolveNav } from "./navitems";
import { MENUS } from "./menus";

const accounts = [
  { id: 1, name: "Checking" },
  { id: 2, name: "Savings" },
] as never;

describe("navCatalog", () => {
  const catalog = navCatalog(accounts);
  const byId = (id: string) => catalog.find((e) => e.id === id);

  it("holds Home, the investments view, every menu item, and every account", () => {
    expect(byId("home")?.label).toBe("Home");
    expect(byId("view.investments")?.disabled).toBe("Planned: Phase 6");
    for (const m of MENUS) for (const i of m.items) expect(byId(i.id), i.id).toBeTruthy();
    expect(byId("account:2")).toMatchObject({ label: "Savings", group: "Accounts" });
    expect(new Set(catalog.map((e) => e.id)).size).toBe(catalog.length);
  });

  it("gives ambiguous menu items a clear button label and keeps the greyed reason", () => {
    expect(byId("reports.investing")?.label).toBe("Investing report");
    expect(byId("file.open")?.label).toBe("Open book");
    expect(byId("edit.settings")?.label).toBe("Settings");
    expect(byId("tools.reconcile")?.disabled).toBeUndefined();
    expect(byId("view.investments")?.disabled).toBe("Planned: Phase 6");
    expect(byId("tools.calendar")?.disabled).toBeUndefined();
  });

  it("the default bar is all in the catalog", () => {
    expect(resolveNav(DEFAULT_NAV, catalog).map((e) => e.id)).toEqual(DEFAULT_NAV);
  });
});

describe("list operations", () => {
  it("resolveNav skips unknown ids (a deleted account) and repeats, keeping the order", () => {
    const catalog = navCatalog(accounts);
    const r = resolveNav(["tools.calendar", "account:9", "home", "home", "account:1"], catalog);
    expect(r.map((e) => e.id)).toEqual(["tools.calendar", "home", "account:1"]);
  });

  it("moveNav swaps with a neighbour and stops at the ends", () => {
    const ids = ["a", "b", "c"];
    expect(moveNav(ids, "b", -1)).toEqual(["b", "a", "c"]);
    expect(moveNav(ids, "b", 1)).toEqual(["a", "c", "b"]);
    expect(moveNav(ids, "a", -1)).toBe(ids);
    expect(moveNav(ids, "c", 1)).toBe(ids);
    expect(moveNav(ids, "zzz", 1)).toBe(ids);
    expect(ids).toEqual(["a", "b", "c"]); // never changed in place
  });

  it("addNav appends once; removeNav removes", () => {
    expect(addNav(["a"], "b")).toEqual(["a", "b"]);
    expect(addNav(["a", "b"], "a")).toEqual(["a", "b"]);
    expect(removeNav(["a", "b", "c"], "b")).toEqual(["a", "c"]);
  });

  it("parseNav accepts a list of strings only", () => {
    expect(parseNav('["a","b"]')).toEqual(["a", "b"]);
    expect(parseNav("")).toBeNull();
    expect(parseNav("{}")).toBeNull();
    expect(parseNav("[1,2]")).toBeNull();
    expect(parseNav("nope")).toBeNull();
  });
});
