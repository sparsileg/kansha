// Timed backups (SET-050). Rust decides when one is due (a delay after the
// first change since the last backup); this asks every so often, says so
// in the status bar before the backup starts, and again when it is done.

import { call, commands } from "../api";
import { attentionState } from "../state/attention.svelte";
import { statusState } from "../state/status.svelte";
import { backupMessage } from "./actions";

/** How often to ask whether a timed backup is due. */
export const CHECK_MS = 30_000;
/** After a failure, checks skipped, so it is not reported every time. */
export const RETRY_AFTER_CHECKS = 10;

let busy = false;
let skip = 0;
let timer: ReturnType<typeof setInterval> | undefined;

/** One check: back up if one is due. Never throws. */
export async function timedBackupCheck(): Promise<void> {
  if (busy) return;
  if (skip > 0) {
    skip--;
    return;
  }
  busy = true;
  try {
    if (!(await call(commands.backupTimedDue()))) return;
    statusState.show("Timed backup starting…");
    const m = backupMessage(await call(commands.backupTimedRun()), "Timed backup finished: ");
    statusState.show(m.text, m.kind);
    attentionState.changed();
  } catch (e) {
    skip = RETRY_AFTER_CHECKS;
    statusState.show(`The timed backup failed: ${e instanceof Error ? e.message : String(e)}`, "alert");
  } finally {
    busy = false;
  }
}

/** Begin checking, once a book is open. Starting again does nothing. */
export function startTimedBackups(): void {
  timer ??= setInterval(() => void timedBackupCheck(), CHECK_MS);
}

/** For tests. */
export function stopTimedBackups(): void {
  clearInterval(timer);
  timer = undefined;
  busy = false;
  skip = 0;
}
