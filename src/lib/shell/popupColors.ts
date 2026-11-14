// Open dropdown lists in the theme (NFR-080, #18): WebKitGTK draws a
// select's open list as a GTK popup that ignores the page's CSS, so the
// theme's option colors and the font size go to Rust, which styles the
// popup (Linux; a no-op elsewhere).

import { commands } from "../api";
import type { PopupColors } from "../types/bindings";

/** The theme's dropdown colors on `root`, or null before the theme's
 * CSS is in. */
export function popupColorsOf(root: HTMLElement, fontPx: number): PopupColors | null {
  const css = getComputedStyle(root);
  const v = (name: string) => css.getPropertyValue(name).trim();
  const colors = { bg: v("--opt-bg"), fg: v("--opt-fg"), sel_bg: v("--sel-bg"), sel_fg: v("--sel-fg") };
  if (Object.values(colors).some((c) => c === "")) return null;
  return { ...colors, font_px: Math.round(fontPx) };
}

/** Send them; outside the app (tests, a browser) nothing happens. */
export async function syncPopupColors(root: HTMLElement, fontPx: number): Promise<void> {
  const colors = popupColorsOf(root, fontPx);
  if (colors === null) return;
  try {
    await commands.popupColors(colors);
  } catch {
    /* not running inside Tauri */
  }
}
