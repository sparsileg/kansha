// The open account's register: which account, the filter/sort/paging
// query, the current page, and the footer summary (REG-010 … REG-060).
// Opens newest-first. A response that arrives after a newer request was
// issued is dropped, so fast typing in a filter cannot show stale rows.

import { call, commands } from "../api";
import type {
  AccountId,
  Cleared,
  TxnId,
  RegisterPage,
  RegisterQuery,
  RegisterSort,
  RegisterSummary,
} from "../types/bindings";
import { listsState } from "./lists.svelte";
import { settingsState } from "./settings.svelte";

/** The user-editable filters (REG-040); everything in the query but the
 * account, sort, and paging. */
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
  descending = $state(true);
  /** Zero-based page index. */
  pageIndex = $state(0);

  page = $state<RegisterPage | null>(null);
  summary = $state<RegisterSummary | null>(null);
  /** Highlighted row, and the row being edited in place (REG-030). */
  selected = $state<TxnId | null>(null);
  editing = $state<TxnId | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);

  #seq = 0;

  rows = $derived(this.page?.rows ?? []);
  total = $derived(this.page?.total ?? 0);
  pageCount = $derived(
    Math.max(1, Math.ceil(this.total / settingsState.pageSize)),
  );
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
      limit: settingsState.pageSize,
      offset: this.pageIndex * settingsState.pageSize,
    };
  }

  /** Switch account: filters reset, register opens newest-first. */
  async open(account: AccountId): Promise<void> {
    this.accountId = account;
    this.filters = emptyFilters();
    this.sort = "date";
    this.descending = true;
    this.pageIndex = 0;
    this.page = null;
    this.summary = null;
    this.selected = null;
    this.editing = null;
    await this.reload();
  }

  /** TXN-030: show the other side of a transfer, next to its entry. */
  async goToTransaction(
    account: AccountId,
    txn: TxnId,
    date: string,
  ): Promise<void> {
    await this.open(account);
    this.selected = txn;
    await this.setFilters({ date_from: date, date_to: date });
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
    this.pageIndex = 0;
    await this.reload();
  }

  async clearFilters(): Promise<void> {
    this.filters = emptyFilters();
    this.pageIndex = 0;
    await this.reload();
  }

  /** Click a column header: same column flips direction; a new one starts
   * ascending, except date, which starts newest-first. */
  async sortBy(column: RegisterSort): Promise<void> {
    if (column === this.sort) {
      this.descending = !this.descending;
    } else {
      this.sort = column;
      this.descending = column === "date";
    }
    this.pageIndex = 0;
    await this.reload();
  }

  async goToPage(index: number): Promise<void> {
    const clamped = Math.min(Math.max(0, index), this.pageCount - 1);
    if (clamped === this.pageIndex && this.page) return;
    this.pageIndex = clamped;
    await this.reload();
  }
}

export const registerState = new RegisterState();
