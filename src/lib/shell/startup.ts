// What happens once a book is open (after the passphrase, setup, or a
// restore): its settings, the reference lists, the startup view, the
// scheduler, and the integrity check if the setting asks for it.

import { bookSettings } from "../state/booksettings.svelte";
import { dialogState } from "../state/dialogs.svelte";
import { investViewState } from "../state/investview.svelte";
import { listsState } from "../state/lists.svelte";
import { scheduleState } from "../state/schedule.svelte";
import { viewState } from "../state/view.svelte";
import { openStartup } from "./nav";

export async function startBook(): Promise<void> {
  await bookSettings.load();
  investViewState.applyStored();
  await listsState.loadAll();
  // Open the startup setting, unless the user has already gone somewhere.
  if (viewState.untouched) await openStartup();
  if (bookSettings.value.integrity_at_startup) dialogState.integrity = true;
  await scheduleState.startup();
}
