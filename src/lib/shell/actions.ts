// What running a menu item or navigation-bar button does, by id. Items
// marked `disabled` never get here (the callers check).

import { call, commands } from "../api";
import { MENU_REPORTS } from "../reports/meta";
import { dialogState } from "../state/dialogs.svelte";
import { reportState } from "../state/reports.svelte";
import { registerState } from "../state/register.svelte";
import { viewState } from "../state/view.svelte";
import { exitApp, goHome, openAccount, openReconcile } from "./nav";
import { openPanel } from "./panels";
import { windowState } from "../state/windows.svelte";
import { HOME_ID, INVESTMENTS_ID } from "./navitems";

export function runAction(id: string): void {
  if (id === HOME_ID) {
    goHome();
  } else if (id.startsWith("account:")) {
    void openAccount(Number(id.slice("account:".length)));
  } else if (id in MENU_REPORTS) {
    void reportState.open(MENU_REPORTS[id]);
  } else {
    switch (id) {
      case "file.backup":
        void backUpNow();
        break;
      case "file.restore":
        dialogState.restore = true;
        break;
      case "file.integrity":
        dialogState.integrity = true;
        break;
      case "file.exit":
        void exitApp();
        break;
      case "edit.settings":
        dialogState.settings = true;
        break;
      case "edit.navbar":
        dialogState.navbar = true;
        break;
      case "tools.accounts":
        openPanel("accounts");
        break;
      case "tools.calendar":
        openPanel("calendar");
        break;
      case "tools.reminders":
        openPanel("scheduled");
        break;
      case "tools.reconcile":
        void openReconcile();
        break;
      case "tools.payees":
        viewState.navigate("manage", { tab: "payees" });
        break;
      case "tools.categories":
        viewState.navigate("manage", { tab: "categories" });
        break;
      case "tools.tags":
        viewState.navigate("manage", { tab: "tags" });
        break;
      case "tools.securities":
        viewState.navigate("manage", { tab: "securities" });
        break;
      case INVESTMENTS_ID:
        viewState.navigate("investments");
        break;
      case "reports.saved":
        reportState.savedOpen = true;
        break;
    }
  }
}

/** File > Back Up Now (BAK-030): Rust picks the folder; say where it went. */
export async function backUpNow(): Promise<void> {
  try {
    const r = await call(commands.backupNow());
    const lines = [`Backed up to ${r.path}.`];
    if (r.folder_missing) lines.push("⚠ The backup folder is missing, so the backup went to Downloads. Choose a folder in Settings.");
    if (r.integrity_issues > 0) lines.push(`⚠ The integrity check found ${r.integrity_issues} problem(s) in the backed-up data.`);
    dialogState.backupDone = { ok: true, text: lines.join(" ") };
  } catch (e) {
    dialogState.backupDone = { ok: false, text: `The backup failed: ${e instanceof Error ? e.message : String(e)}` };
  }
}

/** Whether the screen showing now is what this item opens (for the
 * navigation bar's current-page mark). */
export function isCurrent(id: string): boolean {
  const view = viewState.current;
  if (id.startsWith("account:")) {
    return view === "account" && registerState.accountId === Number(id.slice("account:".length));
  }
  switch (id) {
    case "tools.accounts":
      return windowState.shownKind === "accounts";
    case "tools.calendar":
      return windowState.shownKind === "calendar";
    case "tools.reminders":
      return windowState.shownKind === "scheduled";
    case "tools.reconcile":
      return windowState.shownKind === "reconcile";
    case "tools.payees":
    case "tools.categories":
    case "tools.tags":
    case "tools.securities":
      return view === "manage" && viewState.params.tab === id.slice("tools.".length);
    case INVESTMENTS_ID:
      return view === "investments";
    default:
      if (id in MENU_REPORTS) return reportState.current?.kind === MENU_REPORTS[id];
      return false;
  }
}
