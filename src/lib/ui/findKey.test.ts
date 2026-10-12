import { afterEach, describe, expect, it } from "vitest";
import { focusSearch, installFindKey, isFindKey } from "./findKey";

const key = (over: Partial<KeyboardEvent>) =>
  ({ key: "f", ctrlKey: true, metaKey: false, altKey: false, shiftKey: false, ...over }) as KeyboardEvent;

function searchBox(text: string): HTMLInputElement {
  const box = document.createElement("input");
  box.type = "search";
  box.setAttribute("aria-label", "Search");
  box.value = text;
  document.body.append(box);
  return box;
}

describe("find key (UI-050)", () => {
  afterEach(() => document.body.replaceChildren());

  it("is Ctrl+F or Cmd+F", () => {
    expect(isFindKey(key({}))).toBe(true);
    expect(isFindKey(key({ key: "F" }))).toBe(true);
    expect(isFindKey(key({ ctrlKey: false, metaKey: true }))).toBe(true);
    expect(isFindKey(key({ shiftKey: true }))).toBe(false);
    expect(isFindKey(key({ altKey: true }))).toBe(false);
    expect(isFindKey(key({ ctrlKey: false }))).toBe(false);
    expect(isFindKey(key({ key: "g" }))).toBe(false);
  });

  it("puts the cursor in the Search box with its text selected, from a text field too", () => {
    const box = searchBox("rent");
    const field = document.createElement("input");
    document.body.append(field);
    field.focus();
    const off = installFindKey(document);
    const e = new KeyboardEvent("keydown", { key: "f", ctrlKey: true, bubbles: true, cancelable: true });
    field.dispatchEvent(e);
    expect(e.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(box);
    expect([box.selectionStart, box.selectionEnd]).toEqual([0, 4]);
    off();
  });

  it("does nothing with a modal open or no Search box", () => {
    expect(focusSearch(document)).toBe(false);
    const box = searchBox("");
    const modal = document.createElement("div");
    modal.setAttribute("aria-modal", "true");
    document.body.append(modal);
    expect(focusSearch(document)).toBe(false);
    expect(document.activeElement).not.toBe(box);
    modal.remove();
    expect(focusSearch(document)).toBe(true);
  });
});
