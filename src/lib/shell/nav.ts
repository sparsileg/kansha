// Shell navigation helpers shared by the menu bar, the navigation bar, and
// the account list.

import { listsState } from "../state/lists.svelte";
import { registerState } from "../state/register.svelte";
import { settingsState } from "../state/settings.svelte";
import { viewState } from "../state/view.svelte";
import type { AccountId } from "../types/bindings";

/** Show an account's register. Coming back to the account already open
 * keeps its filters and sort; a different one starts fresh. */
export async function openAccount(id: AccountId): Promise<void> {
  viewState.navigate("account");
  if (id !== registerState.accountId) await registerState.open(id);
}

/** The home screen setting, as a place to go. */
export async function goHome(): Promise<void> {
  const home = settingsState.home;
  if (home.startsWith("account:")) {
    const id = Number(home.slice("account:".length));
    if (listsState.account(id)) {
      await openAccount(id);
      return;
    }
  } else if (home === "calendar") {
    viewState.navigate("calendar");
    return;
  } else if (home === "scheduled") {
    viewState.navigate("scheduled");
    return;
  }
  viewState.navigate("dashboard");
}

/** Close the window (File > Exit). */
export async function exitApp(): Promise<void> {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close();
  } catch {
    /* not running inside Tauri */
  }
}
