// Whether a book is open, and what the start screen shows until then
// (SECU-020, SECU-080, SECU-090). Rust decides; this mirrors it.

import { call, commands } from "../api";
import type { BookStatus } from "../types/bindings";

class BookState {
  status = $state<BookStatus | null>(null);
  error = $state<string | null>(null);

  open = $derived(this.status?.open ?? false);

  async refresh(): Promise<void> {
    try {
      this.status = await call(commands.bookStatus());
      this.error = null;
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
    }
  }
}

export const bookState = new BookState();
