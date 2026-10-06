// Scheduled transactions the UI shows: the list, the due and overdue
// items, and auto-entered items awaiting review (REC-070, REC-130).

import { call, commands } from "../api";
import { displayDate } from "../format/date";
import { formatMoney } from "../format/money";
import type {
  AutoEnterReport,
  OccurrenceView,
  ScheduleFields,
  ScheduleId,
  ScheduleRow,
} from "../types/bindings";
import { confirmState } from "./confirm.svelte";
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
    const row = this.row(v.schedule);
    if (row && this.movesInvestmentCash(row.schedule.fields)) {
      await this.enterAsScheduled(v, row.schedule.fields);
      return;
    }
    const entry = await call(commands.schedulePrefill(v.schedule, v.nominal));
    dialogState.due = false;
    viewState.navigate("account", { account: entry.account });
    await registerState.open(entry.account);
    registerState.prefill = { entry, schedule: v.schedule, due: v.nominal };
  }

  /** A schedule on an investment account, or a transfer into one: its cash
   * in or out is recorded by the investments engine. */
  movesInvestmentCash(f: ScheduleFields): boolean {
    const investment = (id: number) => listsState.account(id)?.investment != null;
    return (
      investment(f.account) ||
      f.lines.some((l) => l.target.kind === "account" && investment(l.target.id))
    );
  }

  /** There is no bank register to edit it in, so ask, then enter it as
   * scheduled and open the investment register with it selected, to
   * change the amount there. A failure is thrown to the caller, as for
   * the register path. */
  private async enterAsScheduled(v: OccurrenceView, f: ScheduleFields): Promise<void> {
    const payee = f.payee === null ? "" : (listsState.payee(f.payee)?.name ?? "");
    const amount = `${formatMoney(v.amount)}${v.estimated ? " (an estimate)" : ""}`;
    const what = `${payee || "this transaction"} for ${amount} on ${displayDate(v.date)}`;
    if (!(await confirmState.ask(`Enter ${what} as scheduled? The investment register opens with it selected, to change it there.`)))
      return;
    const entered = await call(
      commands.scheduleEnter(v.schedule, v.nominal, { date: null, amount: null, entry: null }, null, true),
    );
    await this.changed();
    const account = this.investmentAccount(f);
    dialogState.due = false;
    viewState.navigate("account", { account });
    await registerState.goToTransaction(account, entered.txn);
  }

  /** The investment account whose cash a schedule moves: its own, or the
   * one it transfers to. */
  private investmentAccount(f: ScheduleFields): number {
    if (listsState.account(f.account)?.investment != null) return f.account;
    const line = f.lines.find((l) => l.target.kind === "account" && listsState.account(l.target.id)?.investment != null);
    return line?.target.kind === "account" ? line.target.id : f.account;
  }

  /** After entering, skipping, or editing: lists, balances, open register. */
  async changed(): Promise<void> {
    await this.load();
    await listsState.loadBalances();
    if (registerState.accountId !== null) await registerState.refresh();
  }
}

export const scheduleState = new ScheduleState();
