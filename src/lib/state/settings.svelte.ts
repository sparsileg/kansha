// Non-visual UI settings, kept in the book (SET-070) through
// booksettings.svelte.ts. Theme and font size live in theme.svelte.ts.

import { DEFAULT_NAV, parseNav } from "../shell/navitems";
import { bookSettings } from "./booksettings.svelte";

export type PanelSide = "left" | "right";

class SettingsState {
  /** Session only: the account list's "show closed" toggle. */
  showClosedAccounts = $state(false);
  /** The account list panel: open beside the register, or closed. */
  accountPanelOpen = $derived(bookSettings.value.account_panel_open);
  accountPanelSide = $derived<PanelSide>(bookSettings.value.account_panel_side);
  /** The panel's dragged width in pixels, or 0 for the stock width. */
  accountPanelWidth = $derived(bookSettings.value.account_panel_width);
  /** The width while it is being dragged (not yet stored); else `null`. */
  liveWidth = $state<number | null>(null);
  /** CSS for the panel and the bar above it. */
  accountPanelCss = $derived.by(() => {
    const w = this.liveWidth ?? this.accountPanelWidth;
    return w > 0 ? `${w}px` : "16rem";
  });
  /** Where the app opens at startup: a view, a panel, or "account:<id>"
   * (shell/nav.ts `startupChoices`). */
  startup = $derived(bookSettings.value.startup);
  /** The chosen backup folder; `null` = Downloads (BAK-030). */
  backupFolder = $derived(bookSettings.value.backup_folder);

  /** Navigation bar buttons, left to right (ids from shell/navitems). */
  navItems = $derived<string[]>(parseNav(bookSettings.value.nav_items ?? "") ?? [...DEFAULT_NAV]);

  setNavItems(ids: string[]) {
    void bookSettings.update({ nav_items: JSON.stringify(ids) });
  }
  setAccountPanelOpen(open: boolean) {
    void bookSettings.update({ account_panel_open: open });
  }
  setAccountPanelSide(side: PanelSide) {
    void bookSettings.update({ account_panel_side: side });
  }
  setAccountPanelWidth(px: number) {
    void bookSettings.update({ account_panel_width: px });
  }
  setStartup(startup: string) {
    void bookSettings.update({ startup });
  }
}

export const settingsState = new SettingsState();
