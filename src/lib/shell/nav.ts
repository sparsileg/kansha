// Shell navigation helpers shared by the menu bar, the navigation bar, and
// the account list.

import { isReconcilable } from "../reconcile/form";
import { listsState } from "../state/lists.svelte";
import { reconcileState } from "../state/reconcile.svelte";
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

/** Show the Reconcile view for an account: the one given, else the one
 * whose register is open, else the last one reconciled, else the first
 * that can be. */
export async function openReconcile(id?: AccountId): Promise<void> {
  const usable = (a: AccountId | null | undefined): a is AccountId => {
    const acct = a == null ? undefined : listsState.account(a);
    return acct !== undefined && acct.status === "open" && isReconcilable(acct.account_type);
  };
  const first = listsState.accounts.find((a) => usable(a.id))?.id;
  const target = [id, registerState.accountId, reconcileState.accountId, first].find(usable) ?? null;
  viewState.navigate("reconcile");
  if (target !== reconcileState.accountId || reconcileState.session === null) {
    await reconcileState.select(target);
  }
}
