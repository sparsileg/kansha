import { describe, expect, it } from "vitest";
import { moveSelection, rowKeyAction } from "./keys";

const k = (key: string, o: object = {}) => ({
  key, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...o,
});

describe("rowKeyAction", () => {
  it("maps register keys", () => {
    expect(rowKeyAction(k("ArrowDown"))).toBe("down");
    expect(rowKeyAction(k("Enter"))).toBe("edit");
    expect(rowKeyAction(k("Delete"))).toBe("delete");
    expect(rowKeyAction(k(" "))).toBe("toggle-clear");
    expect(rowKeyAction(k("Insert"))).toBe("new");
    expect(rowKeyAction(k("n", { ctrlKey: true }))).toBe("new");
    expect(rowKeyAction(k("F10", { shiftKey: true }))).toBe("menu");
  });
  it("ignores other keys and modified keys", () => {
    expect(rowKeyAction(k("a"))).toBeNull();
    expect(rowKeyAction(k("Enter", { altKey: true }))).toBeNull();
    expect(rowKeyAction(k("Delete", { ctrlKey: true }))).toBeNull();
  });
});

describe("moveSelection", () => {
  const ids = [10, 20, 30, 40, 50];
  it("moves and clamps", () => {
    expect(moveSelection(ids, 20, "down")).toBe(30);
    expect(moveSelection(ids, 50, "down")).toBe(50);
    expect(moveSelection(ids, 10, "up")).toBe(10);
    expect(moveSelection(ids, null, "down")).toBe(10);
    expect(moveSelection(ids, 30, "first")).toBe(10);
    expect(moveSelection(ids, 30, "last")).toBe(50);
    expect(moveSelection(ids, 30, "page-down", 2)).toBe(50);
    expect(moveSelection(ids, 30, "page-up", 5)).toBe(10);
  });
  it("returns null for no rows", () => {
    expect(moveSelection([], null, "down")).toBeNull();
  });
});
