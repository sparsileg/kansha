import { afterEach, describe, expect, it } from "vitest";
import { selectOnFocus } from "./selectOnFocus";

let field: HTMLInputElement | HTMLTextAreaElement;
let stop: () => void;

function setup(value: string, el: HTMLInputElement | HTMLTextAreaElement = document.createElement("input")) {
  field = el;
  field.value = value;
  document.body.append(field);
  stop = selectOnFocus(document);
}

afterEach(() => {
  stop();
  field.remove();
});

describe("selectOnFocus", () => {
  it("selects the whole value when a text field gets focus", () => {
    setup("1,000.00");
    field.focus();
    expect([field.selectionStart, field.selectionEnd]).toEqual([0, 8]);
  });

  it("covers text areas too", () => {
    setup("note", document.createElement("textarea"));
    field.focus();
    expect([field.selectionStart, field.selectionEnd]).toEqual([0, 4]);
  });

  it("keeps the selection through the mouseup of the focusing click, once", () => {
    setup("1,000.00");
    field.focus();
    const first = new MouseEvent("mouseup", { cancelable: true, bubbles: true });
    field.dispatchEvent(first);
    expect(first.defaultPrevented).toBe(true);
    // A later click in the focused field places the caret as usual.
    const second = new MouseEvent("mouseup", { cancelable: true, bubbles: true });
    field.dispatchEvent(second);
    expect(second.defaultPrevented).toBe(false);
  });

  it("stops listening when stopped", () => {
    setup("12.00");
    stop();
    field.focus();
    expect(field.selectionStart).toBe(field.selectionEnd);
  });
});
