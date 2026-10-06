// Shell navigation helpers shared by the menu bar, the navigation bar, and
// the account list.

import { isReconcilable } from "../reconcile/form";
import { listsState } from "../state/lists.svelte";
import { reconcileState } from "../state/reconcile.svelte";
import { registerState } from "../state/register.svelte";
import { settingsState } from "../state/settings.svelte";
import { groupAccounts } from "../state/groups";
import { viewState, type ViewId } from "../state/view.svelte";
import { windowState } from "../state/windows.svelte";
import { isPanel, openPanel, type PanelKind } from "./panels";
import type { AccountId } from "../types/bindings";

/** Show an account's register. Coming back to the account already open
 * keeps its filters and sort; a different one starts fresh. */
export async function openAccount(id: AccountId): Promise<void> {
  viewState.navigate("account", { account: id });
  if (id !== registerState.accountId) await registerState.open(id);
}

/** The Insights view, first tab. */
export function openInsights(): void {
  viewState.navigate("insights");
}

/** One insight's tab in the Insights view. */
export function openInsight(id: number): void {
  viewState.navigate("insights", { insight: id });
}

/** Which views startup can open, by label; `null` for those it cannot
 * (they need an account, a search, or a window). A `Record` over every
 * view and panel, so adding one fails to compile until it is listed. */
const STARTUP_VIEWS: Record<ViewId, string | null> = {
  insights: "Insights",
  account: null, // each account is its own choice
  window: null,
  manage: null,
  search: null,
  settings: null,
};
const STARTUP_PANELS: Record<PanelKind, string | null> = {
  investments: "Investments",
  scheduled: "Reminders",
  calendar: "Calendar",
  accounts: "Accounts",
  reconcile: null,
};

/** The "On startup open to" choices: views, panels, then every account
 * in the account list's order. */
export function startupChoices(): { value: string; label: string }[] {
  const out: { value: string; label: string }[] = [];
  for (const [value, label] of Object.entries({ ...STARTUP_VIEWS, ...STARTUP_PANELS })) {
    if (label !== null) out.push({ value, label });
  }
  for (const g of groupAccounts(listsState.accounts, false)) {
    for (const a of g.accounts) out.push({ value: `account:${a.id}`, label: `Account: ${a.name}` });
  }
  return out;
}

/** Open what the startup setting names; Insights if it names
 * nothing that exists. The history starts there (UI-025). */
export async function openStartup(): Promise<void> {
  await openStartupScreen();
  viewState.startHere();
}

async function openStartupScreen(): Promise<void> {
  const to = settingsState.startup;
  if (to.startsWith("account:")) {
    const id = Number(to.slice("account:".length));
    if (listsState.account(id)) {
      await openAccount(id);
      return;
    }
  } else if (isPanel(to) && STARTUP_PANELS[to] !== null) {
    openPanel(to);
    return;
  } else if (to in STARTUP_VIEWS && STARTUP_VIEWS[to as ViewId] !== null) {
    viewState.navigate(to as ViewId);
    return;
  }
  openInsights();
}

/** Set once the user has agreed to quit, so closing does not ask twice. */
let quitting = false;

/** The title bar's close box asks to save changed reports, as File >
 * Exit does; cancelling keeps the app open. Tauri waits for this handler
 * and closes the window unless it prevents that. */
export async function guardWindowClose(): Promise<void> {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().onCloseRequested(async (e) => {
      if (!quitting && !(await windowState.mayQuit())) e.preventDefault();
    });
  } catch {
    /* not running inside Tauri */
  }
}

/** Close the window (File > Exit), after asking to save changed
 * reports; cancelling any of them keeps the app open. */
export async function exitApp(): Promise<void> {
  if (!(await windowState.mayQuit())) return;
  quitting = true;
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
    return acct !== undefined && acct.status === "open" && isReconcilable(acct);
  };
  const first = listsState.accounts.find((a) => usable(a.id))?.id;
  const target = [id, registerState.accountId, reconcileState.accountId, first].find(usable) ?? null;
  openPanel("reconcile");
  if (target !== reconcileState.accountId || reconcileState.session === null) {
    await reconcileState.select(target);
  }
}
