import { afterEach, describe, expect, it } from "vitest";
import { selectOnFocus } from "./selectOnFocus";

let input: HTMLInputElement;
let action: { destroy(): void };

function setup(value: string) {
  input = document.createElement("input");
  input.value = value;
  document.body.append(input);
  action = selectOnFocus(input);
}

afterEach(() => {
  action.destroy();
  input.remove();
});

describe("selectOnFocus", () => {
  it("selects the whole value when the field gets focus", () => {
    setup("1,000.00");
    input.focus();
    expect([input.selectionStart, input.selectionEnd]).toEqual([0, 8]);
  });

  it("keeps the selection through the mouseup of the focusing click, once", () => {
    setup("1,000.00");
    input.focus();
    const first = new MouseEvent("mouseup", { cancelable: true, bubbles: true });
    input.dispatchEvent(first);
    expect(first.defaultPrevented).toBe(true);
    // A later click in the focused field places the caret as usual.
    const second = new MouseEvent("mouseup", { cancelable: true, bubbles: true });
    input.dispatchEvent(second);
    expect(second.defaultPrevented).toBe(false);
  });

  it("stops listening after destroy", () => {
    setup("12.00");
    action.destroy();
    input.focus();
    expect(input.selectionStart).toBe(input.selectionEnd);
  });
});
