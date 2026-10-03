// The window is small while the start screen asks for the passphrase,
// and grows to the working size once the book is open: the size and
// place from last time (kept by the Rust side), else FULL. A window the
// user maximized is left alone.

import { commands } from "../api";

export const COMPACT = { width: 520, height: 700, minWidth: 460, minHeight: 600 };
export const FULL = { width: 1280, height: 800, minWidth: 900, minHeight: 600 };

type Size = typeof COMPACT;

/** Whether the window is compact now, so growing happens only after
 * shrinking (a book opened in a full-size window keeps its size). */
let compact = true;

async function resize(s: Size, restored = false): Promise<void> {
  try {
    const { getCurrentWindow, LogicalSize } = await import("@tauri-apps/api/window");
    const w = getCurrentWindow();
    if (restored) {
      // Size and place are back already; only the minimum is left.
      await w.setMinSize(new LogicalSize(s.minWidth, s.minHeight));
      return;
    }
    if (await w.isMaximized()) return;
    // Lower the minimum before shrinking; raise it after growing.
    if (s.minWidth < FULL.minWidth) await w.setMinSize(new LogicalSize(s.minWidth, s.minHeight));
    await w.setSize(new LogicalSize(s.width, s.height));
    if (s.minWidth >= FULL.minWidth) await w.setMinSize(new LogicalSize(s.minWidth, s.minHeight));
    await w.center();
  } catch {
    /* not running inside Tauri */
  }
}

/** A window command; undefined outside Tauri. */
async function rust<T>(f: () => Promise<T>): Promise<T | undefined> {
  try {
    return await f();
  } catch {
    return undefined;
  }
}

/** Small window, for the passphrase screen. */
export async function compactWindow(): Promise<void> {
  if (compact) return;
  compact = true;
  await rust(() => commands.windowSave());
  await resize(COMPACT);
}

/** Working-size window: the book is open, or a start screen with more to
 * show (setup, restore). */
export async function fullWindow(): Promise<void> {
  if (!compact) return;
  compact = false;
  await resize(FULL, (await rust(() => commands.windowRestore())) === true);
}
