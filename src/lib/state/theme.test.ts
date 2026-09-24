import { describe, expect, it } from "vitest";
import { themeState } from "./theme.svelte";

describe("themeState", () => {
  it("toggles between light and dark", () => {
    const start = themeState.theme;
    themeState.toggle();
    expect(themeState.theme).not.toBe(start);
    themeState.toggle();
    expect(themeState.theme).toBe(start);
  });
});
