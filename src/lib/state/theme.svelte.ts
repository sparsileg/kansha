// Theme and base font size state (SET-010, SET-020). Per computer, not in
// the book (SET-070): Rust keeps them in a config file in the OS
// configuration folder (`appearance_get` / `appearance_set`), so the
// passphrase screen can use them before any book is open. With nothing
// stored, the OS light/dark preference picks the theme.
//
// The theme's colors and font family live in src/css/themes/<theme>.css;
// the base size goes on <html>, so every rem in the app scales with it.

import { commands } from "../api";

export type Theme = "light" | "dark" | "classic";

export const THEMES: { value: Theme; label: string }[] = [
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "classic", label: "Classic" },
];

const isTheme = (v: unknown): v is Theme => THEMES.some((t) => t.value === v);

const prefersDark =
  typeof window !== "undefined" &&
  window.matchMedia?.("(prefers-color-scheme: dark)").matches;

export const MIN_FONT_SIZE = 10;
export const MAX_FONT_SIZE = 24;
export const DEFAULT_FONT_SIZE = 13;

/** Every base size offered, in 1px steps. */
export const FONT_SIZES = Array.from({ length: MAX_FONT_SIZE - MIN_FONT_SIZE + 1 }, (_, i) => MIN_FONT_SIZE + i);

const isFontSize = (v: unknown): v is number =>
  typeof v === "number" && Number.isInteger(v) && v >= MIN_FONT_SIZE && v <= MAX_FONT_SIZE;

class ThemeState {
  theme = $state<Theme>(prefersDark ? "dark" : "light");
  /** Base font size in px, set on <html>: 1rem (NFR-080). */
  fontSize = $state(DEFAULT_FONT_SIZE);

  /** Read this computer's stored choice. Never fails: the defaults stay. */
  async load(): Promise<void> {
    try {
      const a = await commands.appearanceGet();
      if (isTheme(a.theme)) this.theme = a.theme;
      if (isFontSize(a.font_size)) this.fontSize = a.font_size;
    } catch {
      /* not running inside Tauri */
    }
  }

  toggle() {
    this.setTheme(this.theme === "light" ? "dark" : "light");
  }
  setTheme(theme: Theme) {
    if (!isTheme(theme)) return;
    this.theme = theme;
    this.#save();
  }
  setFontSize(px: number) {
    if (!isFontSize(px)) return;
    this.fontSize = px;
    this.#save();
  }

  #save() {
    // Not saved, it still applies for this session.
    Promise.resolve()
      .then(() => commands.appearanceSet({ theme: this.theme, font_size: this.fontSize }))
      .catch(() => {});
  }
}

export const themeState = new ThemeState();

/** Put the theme and base size on <html>, where the theme files and rem
 * read them. */
export function applyTheme(root: HTMLElement, theme: Theme, fontSize: number) {
  root.dataset.theme = theme;
  root.style.fontSize = `${fontSize}px`;
}
