import { describe, expect, it } from "vitest";
import { applyTheme, FONT_SIZES, themeState } from "./theme.svelte";

describe("themeState", () => {
  it("toggles between light and dark", () => {
    themeState.setTheme("light");
    themeState.toggle();
    expect(themeState.theme).toBe("dark");
    themeState.toggle();
    expect(themeState.theme).toBe("light");
  });

  it("offers Classic and refuses an unknown theme", () => {
    themeState.setTheme("classic");
    expect(themeState.theme).toBe("classic");
    themeState.setTheme("neon" as never);
    expect(themeState.theme).toBe("classic");
  });

  it("offers every base size from 10 to 24 px in 1px steps", () => {
    expect(FONT_SIZES[0]).toBe(10);
    expect(FONT_SIZES.at(-1)).toBe(24);
    expect(FONT_SIZES.every((px, i) => i === 0 || px === FONT_SIZES[i - 1] + 1)).toBe(true);
  });

  it("refuses a base size outside the range", () => {
    themeState.setFontSize(15);
    themeState.setFontSize(9);
    themeState.setFontSize(25);
    themeState.setFontSize(13.5);
    expect(themeState.fontSize).toBe(15);
  });

  it("puts the theme and base size on the root element", () => {
    const root = document.createElement("html");
    applyTheme(root, "classic", 17);
    expect(root.dataset.theme).toBe("classic");
    expect(root.style.fontSize).toBe("17px");
  });
});
