// The window is the start page's size while the start screen asks for
// the passphrase, and the working size once the book is open (or for
// setup and restore). The Rust side keeps both sizes for each screen
// resolution and saves them as the user resizes (SET-070).

import { commands } from "../api";
import type { WindowMode } from "../types/bindings";

async function show(mode: WindowMode): Promise<void> {
  try {
    await commands.windowMode(mode);
  } catch {
    /* not running inside Tauri */
  }
}

/** Start page size, for the passphrase screen. */
export async function compactWindow(): Promise<void> {
  await show("start");
}

/** Working size: the book is open, or a start screen with more to
 * show (setup, restore). */
export async function fullWindow(): Promise<void> {
  await show("working");
}
