import { describe, expect, it, vi } from "vitest";
import { applyTheme, FONT_SIZES, FONTS, themeState } from "./theme.svelte";

describe("default theme", () => {
  for (const dark of [false, true]) {
    it(`is Nordic with nothing stored, when the OS prefers ${dark ? "dark" : "light"}`, async () => {
      vi.resetModules();
      vi.stubGlobal("matchMedia", () => ({ matches: dark }));
      const fresh = await import("./theme.svelte");
      expect(fresh.themeState.theme).toBe("nordic");
      expect(fresh.DEFAULT_THEME).toBe("nordic");
      vi.unstubAllGlobals();
    });
  }

  it("offers Nordic", () => {
    themeState.setTheme("nordic");
    expect(themeState.theme).toBe("nordic");
  });

  it("starts in the System font", async () => {
    vi.resetModules();
    const fresh = await import("./theme.svelte");
    expect(fresh.themeState.font).toBe("system");
  });
});

describe("themeState", () => {
  it("toggles between light and dark", () => {
    themeState.setTheme("light");
    themeState.toggle();
    expect(themeState.theme).toBe("dark");
    themeState.toggle();
    expect(themeState.theme).toBe("light");
  });

  it("offers Matrix", () => {
    themeState.setTheme("matrix");
    expect(themeState.theme).toBe("matrix");
  });

  it("offers Classic and refuses an unknown theme", () => {
    themeState.setTheme("classic");
    expect(themeState.theme).toBe("classic");
    themeState.setTheme("neon" as never);
    expect(themeState.theme).toBe("classic");
  });

  it("offers System, Arial, Verdana, and Courier New, each with a fallback for other systems", () => {
    expect(FONTS.map((f) => f.label)).toEqual(["System", "Arial", "Verdana", "Courier New"]);
    for (const f of FONTS) expect(f.stack.split(",").length).toBeGreaterThanOrEqual(3);
  });

  it("refuses an unknown font", () => {
    themeState.setFont("arial");
    themeState.setFont("papyrus" as never);
    expect(themeState.font).toBe("arial");
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

  it("puts the chosen font's stack on the root element, System when none is given", () => {
    const root = document.createElement("html");
    applyTheme(root, "nordic", 13, "courier");
    expect(root.style.getPropertyValue("--font-ui")).toMatch(/^"Courier New",/);
    applyTheme(root, "nordic", 13);
    expect(root.style.getPropertyValue("--font-ui")).toMatch(/^system-ui,/);
  });
});
