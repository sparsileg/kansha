import { beforeEach, describe, expect, it } from "vitest";
import { loadPref, savePref } from "./prefs";
import { settingsState } from "./settings.svelte";
import { themeState } from "./theme.svelte";

beforeEach(() => localStorage.clear());

describe("prefs", () => {
  it("round-trips a value and falls back when it is missing, malformed, or the wrong kind", () => {
    expect(loadPref("x", 5)).toBe(5);
    savePref("x", 9);
    expect(loadPref("x", 5)).toBe(9);
    localStorage.setItem("kansha.x", "not json");
    expect(loadPref("x", 5)).toBe(5);
    savePref("x", "text");
    expect(loadPref("x", 5)).toBe(5);
    savePref("side", "up");
    expect(loadPref("side", "left", (v) => v === "left" || v === "right")).toBe("left");
  });

  it("settings and theme changes are saved", () => {
    settingsState.setAccountPanelSide("right");
    settingsState.setHome("calendar");
    themeState.setFontSize(20);
    themeState.setTheme("dark");
    expect(loadPref("accountPanelSide", "left")).toBe("right");
    expect(loadPref("home", "dashboard")).toBe("calendar");
    expect(loadPref("fontSize", 16)).toBe(20);
    expect(loadPref("theme", "light")).toBe("dark");
  });

  it("does not throw when storage is unavailable", () => {
    const real = Storage.prototype.setItem;
    Storage.prototype.setItem = () => {
      throw new Error("blocked");
    };
    try {
      expect(() => savePref("x", 1)).not.toThrow();
      expect(() => settingsState.setHome("calendar")).not.toThrow();
      expect(settingsState.home).toBe("calendar");
    } finally {
      Storage.prototype.setItem = real;
    }
  });
});
