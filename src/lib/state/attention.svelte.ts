// The Needs attention card (CARD-030) reloads when a backup or a check
// may have changed what it shows. Also the check of the last backup at
// startup (BAK-080).

import { call, commands } from "../api";
import { statusState } from "./status.svelte";

class AttentionState {
  /** Bumped when the card should reload. */
  stamp = $state(0);

  changed(): void {
    this.stamp++;
  }
}

export const attentionState = new AttentionState();

/** Right after unlocking: fully check the book's last backup with the
 * passphrase just typed (Rust records the result). A failure flashes in
 * the status bar; either way the card reloads. Never throws. */
export async function verifyLastBackup(passphrase: string): Promise<void> {
  try {
    const r = await call(commands.backupVerifyLatest(passphrase));
    if (r?.error) statusState.show(`The check of the last backup failed: ${r.error}.`, "alert");
  } catch (e) {
    statusState.show(`The check of the last backup failed: ${e instanceof Error ? e.message : String(e)}`, "alert");
  } finally {
    attentionState.changed();
  }
}
