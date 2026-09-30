// The open account's register: which account, the filter and sort, every
// matching row, and the footer summary (REG-010 … REG-060). All rows are
// loaded (no paging); the grid draws only the ones in view, so the
// register scrolls continuously. Opens date-ascending, scrolled to the
// bottom, so the newest entries sit next to the entry row (Quicken
// style). A response that arrives after a newer request was issued is
// dropped, so fast typing in a filter cannot show stale rows.

import { call, commands } from "../api";
import type {
  AccountId,
  Cleared,
  Entry,
  ScheduleId,
  TxnId,
  RegisterPage,
  RegisterQuery,
  RegisterSort,
  RegisterSummary,
} from "../types/bindings";
import { listsState } from "./lists.svelte";

/** The user-editable filters (REG-040); everything in the query but the
 * account and sort. */
export type RegisterFilters = Pick<
  RegisterQuery,
  "date_from" | "date_to" | "payee" | "category" | "tag" | "cleared" | "text"
>;

export function emptyFilters(): RegisterFilters {
  return {
    date_from: null,
    date_to: null,
    payee: null,
    category: null,
    tag: null,
    cleared: null as Cleared | null,
    text: null,
  };
}

class RegisterState {
  accountId = $state<AccountId | null>(null);
  filters = $state<RegisterFilters>(emptyFilters());
  sort = $state<RegisterSort>("date");
  descending = $state(false);

  page = $state<RegisterPage | null>(null);
  summary = $state<RegisterSummary | null>(null);
  /** Highlighted row, and the row being edited in place (REG-030). */
  selected = $state<TxnId | null>(null);
  editing = $state<TxnId | null>(null);
  /** Open the edit with the split lines showing (context menu Split,
   * REG-080). */
  editSplit = $state(false);
  loading = $state(false);
  /** A scheduled occurrence to enter: the new-entry row takes the entry as
   * its draft, then clears this (REC-110). */
  prefill = $state<{ entry: Entry; schedule: ScheduleId; due: string } | null>(null);
  error = $state<string | null>(null);

  #seq = 0;

  rows = $derived(this.page?.rows ?? []);
  total = $derived(this.page?.total ?? 0);
  filtered = $derived(
    Object.values(this.filters).some((v) => v !== null && v !== ""),
  );

  query(): RegisterQuery | null {
    if (this.accountId === null) return null;
    return {
      account: this.accountId,
      ...$state.snapshot(this.filters),
      sort: this.sort,
      descending: this.descending,
      limit: null,
      offset: 0,
    };
  }

  /** A row to keep fully in view (after a save or a move) until the user
   * scrolls or navigates; the grid re-applies it when its size changes. */
  reveal = $state<TxnId | null>(null);

  /** Set on open; the grid scrolls to the bottom once, then clears it. */
  scrollToEnd = $state(false);

  /** Switch account: filters reset; opens date-ascending at the bottom. */
  async open(account: AccountId): Promise<void> {
    this.accountId = account;
    this.filters = emptyFilters();
    this.sort = "date";
    this.descending = false;
    this.page = null;
    this.summary = null;
    this.selected = null;
    this.editing = null;
    this.reveal = null;
    await this.reload();
    this.scrollToEnd = true;
  }

  /** TXN-030 and drill-downs: open the account with `txn` selected and
   * scrolled into view among its neighbors (no date filter). */
  async goToTransaction(account: AccountId, txn: TxnId): Promise<void> {
    await this.open(account);
    this.scrollToEnd = false;
    this.selected = txn;
    this.reveal = txn;
  }

  close(): void {
    this.#seq++;
    this.accountId = null;
    this.page = null;
    this.summary = null;
    this.selected = null;
    this.editing = null;
    this.loading = false;
  }

  async reload(): Promise<void> {
    const query = this.query();
    if (query === null) return;
    const seq = ++this.#seq;
    this.loading = true;
    try {
      const [page, summary] = await Promise.all([
        call(commands.registerQuery(query)),
        call(commands.registerSummary(query.account)),
      ]);
      if (seq !== this.#seq) return;
      this.page = page;
      this.summary = summary;
      this.error = null;
    } catch (e) {
      if (seq !== this.#seq) return;
      this.error = e instanceof Error ? e.message : String(e);
    } finally {
      if (seq === this.#seq) this.loading = false;
    }
  }

  /** After a ledger write: rows, footer, and account balances. */
  async refresh(): Promise<void> {
    await Promise.all([this.reload(), listsState.loadBalances()]);
  }

  async setFilters(patch: Partial<RegisterFilters>): Promise<void> {
    this.filters = { ...this.filters, ...patch };
    await this.reload();
  }

  async clearFilters(): Promise<void> {
    this.filters = emptyFilters();
    await this.reload();
  }

  /** Click a column header: same column flips direction; a new one starts
   * ascending. */
  async sortBy(column: RegisterSort): Promise<void> {
    if (column === this.sort) {
      this.descending = !this.descending;
    } else {
      this.sort = column;
      this.descending = false;
    }
    await this.reload();
  }
}

export const registerState = new RegisterState();
