// Entries from the command line and files waiting for review (CLI-040).
// The list opens by itself only when a book opens with entries waiting;
// entries that arrive later change the File menu's count and post a note.

import { call, commands } from "../api";
import type { InboxReport, PendingList, Refused } from "../types/bindings";
import { dialogState } from "./dialogs.svelte";
import { statusState } from "./status.svelte";

class EntriesState {
  /** Entries waiting, for File > Review Entries… (n). */
  count = $state(0);
  list = $state<PendingList>({ entries: [], accounts: [], categories: [] });
  error = $state<string | null>(null);
  /** Inbox files already reported as refused, so focus does not repeat
   * the alert. */
  private reported = new Set<string>();
  private reading = false;
  /** A read was asked for while one ran (files arrived meanwhile). */
  private again = false;
  /** A split opened from the list with Approve and open: when its edit
   * in the register ends, the list opens again if entries still wait. */
  reopenAfter: number | null = null;

  /** The register's edit of `txn` ended (saved or not). */
  async editEnded(txn: number): Promise<void> {
    if (this.reopenAfter !== txn) return;
    this.reopenAfter = null;
    await this.refreshCount();
    if (this.count > 0) dialogState.reviewEntries = true;
  }

  async refreshCount(): Promise<void> {
    try {
      this.count = await call(commands.entriesCount());
    } catch {
      /* the count is a hint; the list shows any error */
    }
  }

  async load(): Promise<void> {
    try {
      this.list = await call(commands.entriesPending());
      this.count = this.list.entries.length;
      this.error = null;
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
    }
  }

  /** The book just opened: read the inbox, then open the list when
   * entries wait. */
  async startup(): Promise<void> {
    this.reported.clear();
    await this.readInbox(false);
    await this.refreshCount();
    if (this.count > 0) dialogState.reviewEntries = true;
  }

  /** Read the book's inbox (the book opened, the window got focus, or
   * files arrived). New entries post a note when `note`; a refused file,
   * an alert once. Asked for while a read runs, it runs once after. */
  async readInbox(note = true): Promise<void> {
    if (this.reading) {
      this.again = true;
      return;
    }
    this.reading = true;
    try {
      const report = await call(commands.entriesInboxRead());
      const fresh = report.refused.filter((r) => !this.reported.has(r.file_name));
      for (const r of fresh) this.reported.add(r.file_name);
      if (fresh.length > 0) statusState.show(refusedText(fresh, "stays in the inbox"), "alert");
      else if (note) await this.arrived(added(report));
    } catch (e) {
      statusState.show(`The inbox could not be read: ${e instanceof Error ? e.message : String(e)}`, "alert");
    } finally {
      this.reading = false;
    }
    if (this.again) {
      this.again = false;
      await this.readInbox();
    }
  }

  /** File > Import Entries…: choose a file, receive it, and open the list. */
  async importFile(): Promise<void> {
    const path = await commands.pickEntriesFile(null);
    if (path === null) return;
    try {
      const report = await call(commands.entriesImport(path));
      if (report.refused.length > 0) {
        statusState.show(refusedText(report.refused, "was not imported"), "alert");
        return;
      }
      const n = added(report);
      const skipped = report.received.reduce((s, r) => s + r.skipped, 0);
      statusState.show(
        `Imported ${n} entr${n === 1 ? "y" : "ies"} for review${skipped > 0 ? `; ${skipped} seen before were skipped` : ""}.`,
      );
      await this.refreshCount();
      if (this.count > 0) {
        if (dialogState.reviewEntries) await this.load();
        dialogState.reviewEntries = true;
      }
    } catch (e) {
      statusState.show(`The entries were not imported: ${e instanceof Error ? e.message : String(e)}`, "alert");
    }
  }

  /** `added` entries arrived while the window was open: a note, no dialog. */
  async arrived(added: number): Promise<void> {
    if (added <= 0) return;
    await this.refreshCount();
    if (dialogState.reviewEntries) await this.load();
    statusState.show(
      `${added} new entr${added === 1 ? "y" : "ies"} to review (File > Review Entries…).`,
      "info",
    );
  }
}

export const entriesState = new EntriesState();

function added(r: InboxReport): number {
  return r.received.reduce((s, x) => s + x.added, 0);
}

/** A refused file and its first problems, for the status bar. */
export function refusedText(refused: Refused[], fate: string): string {
  return refused
    .map((r) => {
      const shown = r.errors.slice(0, 3).map((e) => (e.line > 0 ? `line ${e.line}: ${e.message}` : e.message));
      const more = r.errors.length > 3 ? ` (and ${r.errors.length - 3} more)` : "";
      return `${r.file_name} ${fate}: ${shown.join("; ")}${more}.`;
    })
    .join(" ");
}

/** The File menu label with the count of entries waiting. */
export function reviewLabel(count: number): string {
  return count > 0 ? `Review Entries… (${count})` : "Review Entries…";
}
