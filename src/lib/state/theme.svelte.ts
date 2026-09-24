// Theme and font-size state (SET-010, SET-020). Kept in localStorage until
// the `settings` module lands (persisted with the data, per SET-070); the
// first run follows the OS preference.

import { loadPref, savePref } from "./prefs";

export type Theme = "light" | "dark";

const prefersDark =
  typeof window !== "undefined" &&
  window.matchMedia?.("(prefers-color-scheme: dark)").matches;

export const FONT_SIZES = [12, 14, 16, 18, 20, 24];

class ThemeState {
  theme = $state<Theme>(
    loadPref<Theme>("theme", prefersDark ? "dark" : "light", (v) => v === "light" || v === "dark"),
  );
  /** Root font size in px; scales all rem-based UI (NFR-080). */
  fontSize = $state(loadPref<number>("fontSize", 16, (v) => typeof v === "number" && v >= 10 && v <= 32));

  toggle() {
    this.setTheme(this.theme === "light" ? "dark" : "light");
  }
  setTheme(theme: Theme) {
    this.theme = theme;
    savePref("theme", theme);
  }
  setFontSize(px: number) {
    this.fontSize = px;
    savePref("fontSize", px);
  }
}

export const themeState = new ThemeState();
