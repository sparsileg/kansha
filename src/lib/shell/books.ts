// Switching books (File > New, Open, the recent list, Rename Book). Rust
// closes the current book (backed up) and points at the other one; the
// page then reloads so every store starts clean for it (one book open at
// a time, UI-conventions "Books").

import { call, commands } from "../api";
import { windowState } from "../state/windows.svelte";
import type { RecentBook } from "../types/bindings";

/** Start over for the book Rust now points at. */
export function reloadForBook(): void {
  window.location.reload();
}

/** Close the current book and go to the book at `path` (its passphrase
 * is asked next). A changed report is offered for saving first; Cancel
 * stops the switch. */
export async function switchBook(path: string): Promise<void> {
  if (!(await windowState.mayQuit())) return;
  await call(commands.bookOpen(path));
  reloadForBook();
}

/** File > Open…: pick a book's database file and switch to it. */
export async function openBookFile(startFolder: string | null): Promise<void> {
  const path = await commands.pickBookFile(startFolder);
  if (path !== null) await switchBook(path);
}

/** Menu item id for a recent book. */
export const RECENT_PREFIX = "book:";

/** The File > Recent submenu's items. */
export function recentItems(books: RecentBook[]) {
  return books.map((b) => ({
    id: `${RECENT_PREFIX}${b.path}`,
    label: b.current ? `${b.name} (open)` : b.name,
    disabled: !b.exists
      ? "The file is no longer there"
      : b.current
        ? "This book is open"
        : undefined,
  }));
}
