// What running a menu item or navigation-bar button does, by id. Items
// marked `disabled` never get here (the callers check).

import { call, commands, DECLINED, withConfirmation } from "../api";
import { confirmState } from "../state/confirm.svelte";
import { investState } from "../state/invest.svelte";
import { listsState } from "../state/lists.svelte";
import { scheduleState } from "../state/schedule.svelte";
import { MENU_REPORTS } from "../reports/meta";
import { dialogState } from "../state/dialogs.svelte";
import { reportState } from "../state/reports.svelte";
import { registerState } from "../state/register.svelte";
import { statusState, type StatusKind } from "../state/status.svelte";
import { viewState } from "../state/view.svelte";
import { exitApp, goHome, openAccount, openInsight, openReconcile } from "./nav";
import { openPanel } from "./panels";
import { windowState } from "../state/windows.svelte";
import { HOME_ID, INSIGHT_PREFIX, INVESTMENTS_ID } from "./navitems";
import { SAVED_PREFIX } from "./menus";
import type { BackupResult } from "../types/bindings";
import { bookState } from "../state/book.svelte";
import { openBookFile, RECENT_PREFIX, switchBook } from "./books";

export function runAction(id: string): void {
  if (id === HOME_ID || id === "tools.insights") {
    goHome();
  } else if (id.startsWith(INSIGHT_PREFIX)) {
    openInsight(Number(id.slice(INSIGHT_PREFIX.length)));
  } else if (id.startsWith("account:")) {
    void openAccount(Number(id.slice("account:".length)));
  } else if (id.startsWith(RECENT_PREFIX)) {
    void switchBook(id.slice(RECENT_PREFIX.length)).catch(showError);
  } else if (id.startsWith(SAVED_PREFIX)) {
    void reportState.openSavedId(Number(id.slice(SAVED_PREFIX.length))).catch(showError);
  } else if (id in MENU_REPORTS) {
    void reportState.open(MENU_REPORTS[id]);
  } else {
    switch (id) {
      case "file.new":
        dialogState.newBook = true;
        break;
      case "file.open":
        void openBookFile(bookState.status?.folder ?? null).catch(showError);
        break;
      case "file.rename":
        dialogState.renameBook = true;
        break;
      case "file.backup":
        void backUpNow();
        break;
      case "file.restore":
        dialogState.restore = true;
        break;
      case "file.import":
        dialogState.qifImport = true;
        break;
      case "file.integrity":
        dialogState.integrityReport = null;
        dialogState.integrity = true;
        break;
      case "file.exit":
        void exitApp();
        break;
      case "help.about":
        dialogState.about = true;
        break;
      case "edit.undo":
        void undoLast();
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
      case "tools.import_prices":
        dialogState.priceImport = true;
        break;
      case "tools.investments":
      case INVESTMENTS_ID:
        openPanel("investments");
        break;
      case "reports.saved":
        reportState.savedOpen = true;
        break;
    }
  }
}

/** What to tell the user about a backup just made: a note, or an alert
 * when the folder was missing or the data had integrity problems. */
/** A failed action, in the status bar. */
function showError(e: unknown): void {
  statusState.show(e instanceof Error ? e.message : String(e), "alert");
}

export function backupMessage(r: BackupResult, done: string): { text: string; kind: StatusKind } {
  const lines = [`${done}${r.path}.`];
  if (r.folder_missing) lines.push("The backup folder is missing, so the backup went to Downloads. Choose a folder in Settings.");
  if (r.integrity_issues > 0) lines.push(`The integrity check found ${r.integrity_issues} problem(s) in the backed-up data.`);
  return { text: lines.join(" "), kind: lines.length > 1 ? "alert" : "info" };
}

/** File > Back Up Now (BAK-030): Rust picks the folder; say where it went
 * in the status bar. Trouble flashes. */
export async function backUpNow(): Promise<void> {
  try {
    const m = backupMessage(await call(commands.backupNow()), "Backed up to ");
    statusState.show(m.text, m.kind);
  } catch (e) {
    statusState.show(`The backup failed: ${e instanceof Error ? e.message : String(e)}`, "alert");
  }
}

/** Edit > Undo and Ctrl+Z (UI-060): undo the last register change, then
 * refresh what shows it. The status bar says what happened. */
export async function undoLast(): Promise<void> {
  try {
    const label = await call(commands.undoStatus());
    if (label === null) {
      statusState.show("There is nothing to undo.", "info");
      return;
    }
    const done = await withConfirmation((c) => commands.undoApply(c), confirmState.ask);
    if (done === DECLINED) return;
    statusState.show(`Undone: ${label}.`, "info");
  } catch (e) {
    statusState.show(e instanceof Error ? e.message : String(e), "alert");
  }
  await Promise.all([
    scheduleState.changed(),
    investState.accountId !== null ? investState.refresh() : listsState.loadBalances(),
  ]);
}

/** The Investments view's Download Prices (PRC-040): prices from the
 * internet for `date` (the latest when it is today), when Settings
 * allows it. Tickers that got none are named. */
export async function downloadPrices(date: string): Promise<void> {
  statusState.show("Downloading prices…", "info");
  try {
    const r = await call(commands.pricesDownload(date));
    const failed = r.failed.map((f) => `${f.ticker} (${f.reason})`).join(", ");
    statusState.show(
      `Downloaded ${r.stored} price${r.stored === 1 ? "" : "s"}.${failed ? ` No price for ${failed}.` : ""}`,
      failed ? "alert" : "info",
    );
    await investState.loadSecurities();
    if (investState.accountId !== null) await investState.refresh();
    else await listsState.loadBalances();
  } catch (e) {
    statusState.show(`Price download failed: ${e instanceof Error ? e.message : String(e)}`, "alert");
  }
}

/** Whether the screen showing now is what this item opens (for the
 * navigation bar's current-page mark). */
export function isCurrent(id: string): boolean {
  const view = viewState.current;
  if (id.startsWith("account:")) {
    return view === "account" && registerState.accountId === Number(id.slice("account:".length));
  }
  if (id.startsWith(INSIGHT_PREFIX)) {
    const insight = Number(id.slice(INSIGHT_PREFIX.length));
    return view === "dashboard" && windowState.shown === null && viewState.params.insight === insight;
  }
  switch (id) {
    case HOME_ID:
    case "tools.insights":
      return view === "dashboard" && windowState.shown === null;
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
    case "tools.investments":
    case INVESTMENTS_ID:
      return windowState.shownKind === "investments";
    default:
      if (id in MENU_REPORTS) return reportState.current?.kind === MENU_REPORTS[id];
      return false;
  }
}
