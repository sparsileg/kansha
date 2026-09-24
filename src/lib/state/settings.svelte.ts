// Non-visual UI settings. Theme and font size live in theme.svelte.ts.
// Kept in localStorage until the `settings` module persists them with the
// book (SET-070); see prefs.ts.

import { DEFAULT_NAV, parseNav } from "../shell/navitems";
import { loadPref, savePref } from "./prefs";

export type PanelSide = "left" | "right";

class SettingsState {
  /** Register page size (NFR-040). */
  pageSize = $state(100);
  showClosedAccounts = $state(false);
  /** The account list panel: open beside the register, or closed. */
  accountPanelOpen = $state(loadPref<boolean>("accountPanelOpen", true));
  accountPanelSide = $state<PanelSide>(
    loadPref<PanelSide>("accountPanelSide", "left", (v) => v === "left" || v === "right"),
  );
  /** The home screen: "dashboard", "calendar", "scheduled", or
   * "account:<id>". Opened at startup and by the Home button. */
  home = $state(loadPref<string>("home", "dashboard"));

  /** Navigation bar buttons, left to right (ids from shell/navitems). */
  navItems = $state<string[]>(parseNav(loadPref<string>("navItems", "")) ?? [...DEFAULT_NAV]);

  setNavItems(ids: string[]) {
    this.navItems = ids;
    savePref("navItems", JSON.stringify(ids));
  }
  setAccountPanelOpen(open: boolean) {
    this.accountPanelOpen = open;
    savePref("accountPanelOpen", open);
  }
  setAccountPanelSide(side: PanelSide) {
    this.accountPanelSide = side;
    savePref("accountPanelSide", side);
  }
  setHome(home: string) {
    this.home = home;
    savePref("home", home);
  }
}

export const settingsState = new SettingsState();
