// Theme and base font size state (SET-010, SET-020). Per computer, not in
// the book (SET-070): Rust keeps them in a config file in the OS
// configuration folder (`appearance_get` / `appearance_set`), so the
// passphrase screen can use them before any book is open. With nothing
// stored the theme is Nordic.
//
// The theme's colors live in src/css/themes/<theme>.css. The font (SET-025)
// and the base size go on <html>, so every rem in the app scales with the
// size and every theme shows in the chosen font.

import { commands } from "../api";

export type Theme = "light" | "dark" | "classic" | "matrix" | "nordic";

export const THEMES: { value: Theme; label: string }[] = [
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "classic", label: "Classic" },
  { value: "matrix", label: "Matrix" },
  { value: "nordic", label: "Nordic" },
];

const isTheme = (v: unknown): v is Theme => THEMES.some((t) => t.value === v);

export type Font = "system" | "arial" | "verdana" | "courier";

/** The fonts offered: each falls back to the nearest analogue where the
 * named one is missing (Windows, macOS, and Linux all have one). */
export const FONTS: { value: Font; label: string; stack: string }[] = [
  { value: "system", label: "System", stack: 'system-ui, "Noto Sans", "DejaVu Sans", sans-serif' },
  { value: "arial", label: "Arial", stack: 'Arial, "Liberation Sans", Helvetica, sans-serif' },
  { value: "verdana", label: "Verdana", stack: 'Verdana, "DejaVu Sans", sans-serif' },
  { value: "courier", label: "Courier New", stack: '"Courier New", "Liberation Mono", "DejaVu Sans Mono", monospace' },
];

export const DEFAULT_FONT: Font = "system";

const isFont = (v: unknown): v is Font => FONTS.some((f) => f.value === v);

/** The theme with nothing stored, whatever the OS prefers. */
export const DEFAULT_THEME: Theme = "nordic";

export const MIN_FONT_SIZE = 10;
export const MAX_FONT_SIZE = 24;
export const DEFAULT_FONT_SIZE = 13;

/** Every base size offered, in 1px steps. */
export const FONT_SIZES = Array.from({ length: MAX_FONT_SIZE - MIN_FONT_SIZE + 1 }, (_, i) => MIN_FONT_SIZE + i);

const isFontSize = (v: unknown): v is number =>
  typeof v === "number" && Number.isInteger(v) && v >= MIN_FONT_SIZE && v <= MAX_FONT_SIZE;

class ThemeState {
  theme = $state<Theme>(DEFAULT_THEME);
  font = $state<Font>(DEFAULT_FONT);
  /** Base font size in px, set on <html>: 1rem (NFR-080). */
  fontSize = $state(DEFAULT_FONT_SIZE);

  /** Read this computer's stored choice. Never fails: the defaults stay. */
  async load(): Promise<void> {
    try {
      const a = await commands.appearanceGet();
      if (isTheme(a.theme)) this.theme = a.theme;
      // Nordic Courier, a theme for a few days, became Nordic plus a font.
      if (a.theme === "nordic-courier") {
        this.theme = "nordic";
        this.font = "courier";
      }
      if (isFont(a.font)) this.font = a.font;
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
  setFont(font: Font) {
    if (!isFont(font)) return;
    this.font = font;
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
      .then(() => commands.appearanceSet({ theme: this.theme, font: this.font, font_size: this.fontSize }))
      .catch(() => {});
  }
}

export const themeState = new ThemeState();

/** Put the theme, font, and base size on <html>, where the theme files and
 * rem read them. */
export function applyTheme(root: HTMLElement, theme: Theme, fontSize: number, font: Font = DEFAULT_FONT) {
  root.dataset.theme = theme;
  root.style.setProperty("--font-ui", (FONTS.find((f) => f.value === font) ?? FONTS[0]).stack);
  root.style.fontSize = `${fontSize}px`;
}
