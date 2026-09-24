// What running a menu item or navigation-bar button does, by id. Items
// marked `disabled` never get here (the callers check).

import { dialogState } from "../state/dialogs.svelte";
import { registerState } from "../state/register.svelte";
import { viewState } from "../state/view.svelte";
import { exitApp, goHome, openAccount } from "./nav";
import { HOME_ID } from "./navitems";

export function runAction(id: string): void {
  if (id === HOME_ID) {
    void goHome();
  } else if (id.startsWith("account:")) {
    void openAccount(Number(id.slice("account:".length)));
  } else {
    switch (id) {
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
        viewState.navigate("accounts");
        break;
      case "tools.calendar":
        viewState.navigate("calendar");
        break;
      case "tools.reminders":
        viewState.navigate("scheduled");
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
    }
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
      return view === "accounts";
    case "tools.calendar":
      return view === "calendar";
    case "tools.reminders":
      return view === "scheduled";
    case "tools.payees":
    case "tools.categories":
    case "tools.tags":
      return view === "manage" && viewState.params.tab === id.slice("tools.".length);
    default:
      return false;
  }
}
