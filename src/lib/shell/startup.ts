// What happens once a book is open (after the passphrase, setup, or a
// restore): its settings, the reference lists, the startup view, the
// scheduler, and the integrity check if the setting asks for it.

import { call, commands } from "../api";
import { bookSettings } from "../state/booksettings.svelte";
import { dialogState } from "../state/dialogs.svelte";
import { investViewState } from "../state/investview.svelte";
import { listsState } from "../state/lists.svelte";
import { scheduleState } from "../state/schedule.svelte";
import { statusState } from "../state/status.svelte";
import { viewState } from "../state/view.svelte";
import { openStartup } from "./nav";
import { startTimedBackups } from "./timedbackup";

export async function startBook(): Promise<void> {
  await bookSettings.load();
  investViewState.applyStored();
  // Forget memorized payees not used for a while, if the setting says so.
  await call(commands.payeesForgetStale()).catch(() => 0);
  await listsState.loadAll();
  // Open the startup setting, unless the user has already gone somewhere.
  if (viewState.untouched) await openStartup();
  if (bookSettings.value.integrity_at_startup) await autoIntegrityCheck();
  await scheduleState.startup();
  startTimedBackups();
}

/** An integrity check nobody asked for: a clean result is only a note in
 * the status bar; problems open the report. File > Integrity Check always
 * opens the window. */
export async function autoIntegrityCheck(): Promise<void> {
  try {
    const report = await call(commands.integrityCheck());
    if (report.issues.length === 0) {
      statusState.show("Integrity check found no problems with the data.");
    } else {
      dialogState.integrityReport = report;
      dialogState.integrity = true;
    }
  } catch (e) {
    statusState.show(
      `The integrity check could not run: ${e instanceof Error ? e.message : String(e)}`,
      "alert",
    );
  }
}
