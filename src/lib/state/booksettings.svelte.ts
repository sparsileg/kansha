// The book's settings (SET-030 … SET-070), stored in the book by Rust
// (`settings_get` / `settings_set`), so they travel with the data and are
// restored with it. Theme and font size are per computer: theme.svelte.ts.
//
// Until a book opens, the defaults apply. A change applies at once; if
// saving fails, it still applies for this session and `error` says so.

import { call, commands } from "../api";
import type { Settings } from "../types/bindings";

export const DEFAULT_SETTINGS: Settings = {
  date_format: "mdy",
  week_start: "sunday",
  startup: "insights",
  integrity_at_startup: false,
  nav_items: null,
  account_panel_open: true,
  account_panel_side: "left",
  account_panel_width: 0,
  invest_views: null,
  stale_price_days: 7,
  default_lot_method: "fifo",
  price_download: false,
  upcoming_days: 14,
  backup_folder: null,
  backup_keep_last: 10,
  backup_keep_months: 12,
  backup_timeout_minutes: 5,
  gray_reconciled: true,
  recall_payees: true,
  capitalize_names: false,
  auto_memorize_payees: true,
  purge_payees_months: 0,
  warn_out_of_date: true,
  warn_check_reuse: true,
  confirm_save_change: false,
};

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

class BookSettings {
  value = $state<Settings>({ ...DEFAULT_SETTINGS });
  error = $state<string | null>(null);

  async load(): Promise<void> {
    try {
      this.value = await call(commands.settingsGet());
      this.error = null;
    } catch (e) {
      this.error = message(e);
    }
  }

  /** Change some settings and store them. Resolves to the error message
   * when storing failed (the change still applies for this session). */
  async update(patch: Partial<Settings>): Promise<string | null> {
    const next = { ...this.value, ...patch };
    this.value = next;
    try {
      this.value = await call(commands.settingsSet(next));
      this.error = null;
    } catch (e) {
      this.error = message(e);
    }
    return this.error;
  }

  /** Back to the defaults (tests; a closed book). */
  reset() {
    this.value = { ...DEFAULT_SETTINGS };
    this.error = null;
  }
}

export const bookSettings = new BookSettings();
