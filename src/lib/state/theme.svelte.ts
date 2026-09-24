// Theme and font-size state (SET-010, SET-020). Backed by settings once the
// `settings` module lands (persisted with the data, per SET-070); for now
// it's runtime-only, defaulting to the OS preference.

export type Theme = "light" | "dark";

const prefersDark =
  typeof window !== "undefined" &&
  window.matchMedia?.("(prefers-color-scheme: dark)").matches;

class ThemeState {
  theme = $state<Theme>(prefersDark ? "dark" : "light");
  /** Root font size in px; scales all rem-based UI (NFR-080). */
  fontSize = $state(16);

  toggle() {
    this.theme = this.theme === "light" ? "dark" : "light";
  }
}

export const themeState = new ThemeState();
