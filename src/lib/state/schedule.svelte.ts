// Scheduled transactions the UI shows: the list, the due and overdue
// items, and auto-entered items awaiting review (REC-070, REC-130).

import { call, commands } from "../api";
import type {
  AutoEnterReport,
  OccurrenceView,
  ScheduleId,
  ScheduleRow,
} from "../types/bindings";
import { dialogState } from "./dialogs.svelte";
import { listsState } from "./lists.svelte";
import { viewState } from "./view.svelte";
import { registerState } from "./register.svelte";

class ScheduleState {
  rows = $state<ScheduleRow[]>([]);
  due = $state<OccurrenceView[]>([]);
  review = $state<OccurrenceView[]>([]);
  /** Auto-entry failures from the last run; shown until the dialog closes. */
  failures = $state<AutoEnterReport["failed"]>([]);
  error = $state<string | null>(null);

  rowById = $derived(new Map(this.rows.map((r) => [r.schedule.id, r])));

  row(id: ScheduleId): ScheduleRow | undefined {
    return this.rowById.get(id);
  }

  /** Items needing attention: the due badge in the nav. */
  attention = $derived(this.due.length + this.review.length);

  async load(): Promise<void> {
    try {
      const [rows, due, review] = await Promise.all([
        call(commands.scheduleList()),
        call(commands.scheduleDueList()),
        call(commands.scheduleReviewList()),
      ]);
      this.rows = rows;
      this.due = due;
      this.review = review;
      this.error = null;
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
    }
  }

  /** Startup (REC-070, REC-130): enter what auto schedules owe, then open
   * the due dialog if anything needs the user. */
  async startup(): Promise<void> {
    try {
      const report = await call(commands.scheduleAutoEnter());
      this.failures = report.failed;
      if (report.entered.length > 0) await listsState.loadBalances();
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
    }
    await this.load();
    if (this.attention > 0 || this.failures.length > 0) dialogState.due = true;
  }

  /** Enter an occurrence in its account's register: the entry row is
   * prefilled and focused on the amount, so the user can always adjust it
   * before saving (REC-110). Saving records the occurrence as entered. */
  async enterOccurrence(v: OccurrenceView): Promise<void> {
    const entry = await call(commands.schedulePrefill(v.schedule, v.nominal));
    dialogState.due = false;
    viewState.navigate("account");
    await registerState.open(entry.account);
    registerState.prefill = { entry, schedule: v.schedule, due: v.nominal };
  }

  /** After entering, skipping, or editing: lists, balances, open register. */
  async changed(): Promise<void> {
    await this.load();
    await listsState.loadBalances();
    if (registerState.accountId !== null) await registerState.refresh();
  }
}

export const scheduleState = new ScheduleState();
