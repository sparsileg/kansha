import { describe, expect, it, vi } from "vitest";
import { installUndoKey, isUndoKey } from "./undoKey";

const key = (over: Partial<KeyboardEvent>) =>
  ({ key: "z", ctrlKey: true, metaKey: false, altKey: false, shiftKey: false, target: document.body, ...over }) as KeyboardEvent;

describe("undo key (UI-060)", () => {
  it("is Ctrl+Z or Cmd+Z outside text fields", () => {
    expect(isUndoKey(key({}))).toBe(true);
    expect(isUndoKey(key({ ctrlKey: false, metaKey: true }))).toBe(true);
    expect(isUndoKey(key({ shiftKey: true }))).toBe(false);
    expect(isUndoKey(key({ ctrlKey: false }))).toBe(false);
    expect(isUndoKey(key({ key: "y" }))).toBe(false);
    const input = document.createElement("input");
    expect(isUndoKey(key({ target: input }))).toBe(false);
  });

  it("runs undo once per press", () => {
    const undo = vi.fn();
    const off = installUndoKey(document, undo);
    document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    expect(undo).toHaveBeenCalledTimes(1);
    off();
    document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    expect(undo).toHaveBeenCalledTimes(1);
  });
});
