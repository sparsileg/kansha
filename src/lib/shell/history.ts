// Back and Forward (UI-025): walk the view history, reopening what an
// entry needs: a closed window as it was, an account's register.

import { listsState } from "../state/lists.svelte";
import { registerState } from "../state/register.svelte";
import { viewState, type Entry } from "../state/view.svelte";
import { windowState } from "../state/windows.svelte";

const MANAGE_TABS = { payees: "Payees", categories: "Categories", tags: "Tags", securities: "Securities" };

/** An entry that can no longer be shown: a deleted account, a window
 * that cannot be reopened. */
function dead(e: Entry): boolean {
  if (e.view === "account") return e.params.account !== undefined && !listsState.account(e.params.account);
  if (e.view === "window") return e.params.window === undefined || !windowState.canRevive(e.params.window);
  return false;
}

/** What an entry shows, for the arrows' tooltips. */
export function entryName(e: Entry): string {
  switch (e.view) {
    case "insights":
      return "Insights";
    case "account":
      return (e.params.account !== undefined && listsState.account(e.params.account)?.name) || "account";
    case "window":
      return (e.params.window !== undefined && windowState.nameOf(e.params.window)) || "window";
    case "manage":
      return e.params.tab ? MANAGE_TABS[e.params.tab] : "Manage";
    case "search":
      return e.params.q ? `Search "${e.params.q}"` : "Search";
    case "settings":
      return "Settings";
  }
}

/** The nearest entry that can be shown, `delta` (-1 Back, +1
 * Forward) steps at a time. */
function target(delta: -1 | 1): Entry | null {
  for (let i = delta, e = viewState.peek(i); e; i += delta, e = viewState.peek(i)) {
    if (!dead(e)) return e;
  }
  return null;
}

export const canGo = (delta: -1 | 1): boolean => target(delta) !== null;

/** "Back to Checking", or "Back" when there is nowhere to go. */
export function arrowTitle(delta: -1 | 1): string {
  const word = delta < 0 ? "Back" : "Forward";
  const e = target(delta);
  return e ? `${word} to ${entryName(e)}` : word;
}

/** Step Back (-1) or Forward (+1). */
export async function go(delta: -1 | 1): Promise<void> {
  viewState.forget(dead);
  const e = viewState.peek(delta);
  if (!e) return;
  if (e.view === "window" && e.params.window !== undefined) windowState.revive(e.params.window);
  if (delta < 0) viewState.back();
  else viewState.forward();
  const id = viewState.params.account;
  if (viewState.current === "account" && id !== undefined && id !== registerState.accountId) {
    await registerState.open(id);
  }
}
