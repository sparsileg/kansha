import { describe, expect, it } from "vitest";
import {
  availableColumns,
  defaultViews,
  moveItem,
  orderedAccounts,
  parseViews,
  serializeViews,
  shownAccounts,
  shownSecurities,
  toggle,
} from "./views";

describe("investment views", () => {
  it("has five slots: Default, then Custom 2 to Custom 5", () => {
    const s = defaultViews();
    expect(s.views).toHaveLength(5);
    expect(s.views.map((v) => v.name)).toEqual(["Default", "Custom 2", "Custom 3", "Custom 4", "Custom 5"]);
  });

  it("shows every column but Cost Basis at first", () => {
    const v = defaultViews().views[0];
    expect(v.columns).toEqual([
      "ticker", "price", "shares", "market_value", "gain", "day_gain", "day_percent",
    ]);
    expect(availableColumns(v.columns)).toEqual(["cost_basis"]);
  });

  it("round-trips, and unreadable text gives the defaults", () => {
    const s = defaultViews();
    s.views[2].name = "Retirement";
    s.views[2].columns = ["shares", "gain"];
    s.selected = 2;
    expect(parseViews(serializeViews(s))).toEqual(s);
    expect(parseViews("nonsense")).toEqual(defaultViews());
    expect(parseViews("null")).toEqual(defaultViews());
  });

  it("keeps Show closed lots per view; anything but true is off", () => {
    const s = parseViews(JSON.stringify({ views: [{ showClosed: true }, { showClosed: "yes" }, {}] }));
    expect(s.views.map((v) => v.showClosed).slice(0, 4)).toEqual([true, false, false, false]);
    expect(parseViews(serializeViews(s)).views[0].showClosed).toBe(true);
  });

  it("drops unknown or repeated columns and blank names", () => {
    const s = parseViews(
      JSON.stringify({ views: [{ name: "  ", columns: ["shares", "bogus", "shares"] }], selected: 99 }),
    );
    expect(s.views[0].name).toBe("Default");
    expect(s.views[0].columns).toEqual(["shares"]);
    expect(s.selected).toBe(0);
  });

  it("orders accounts, puts new ones last, drops hidden and gone ones", () => {
    const v = { ...defaultViews().views[0], accountOrder: [3, 1, 9], hiddenAccounts: [2] };
    expect(shownAccounts(v, [1, 2, 3, 4])).toEqual([3, 1, 4]);
    expect(orderedAccounts(v, [1, 2, 3, 4])).toEqual([3, 1, 2, 4]);
  });

  it("passes only shown securities, or none when nothing is hidden", () => {
    const v = { ...defaultViews().views[0] };
    expect(shownSecurities(v, [1, 2, 3])).toBeNull();
    v.hiddenSecurities = [2];
    expect(shownSecurities(v, [1, 2, 3])).toEqual([1, 3]);
  });

  it("moves an item and stays put at the ends", () => {
    expect(moveItem(["a", "b", "c"], 1, -1)).toEqual({ list: ["b", "a", "c"], index: 0 });
    expect(moveItem(["a", "b", "c"], 2, 1)).toEqual({ list: ["a", "b", "c"], index: 2 });
  });

  it("toggles a hidden id", () => {
    expect(toggle([], 4, false)).toEqual([4]);
    expect(toggle([4], 4, true)).toEqual([]);
  });
});
