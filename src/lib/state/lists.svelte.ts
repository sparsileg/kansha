// Reference data the whole UI reads: today, accounts and their balances,
// categories, tags. Loaded from IPC; all figures stay canonical strings.

import { call, commands } from "../api";
import type {
  Account,
  AccountBalance,
  AccountId,
  Category,
  CategoryId,
  Payee,
  Tag,
  TagId,
} from "../types/bindings";

class ListsState {
  /** The Rust clock's today (ISO); empty until `loadAll` finishes. */
  today = $state("");
  accounts = $state<Account[]>([]);
  balances = $state<AccountBalance[]>([]);
  categories = $state<Category[]>([]);
  tags = $state<Tag[]>([]);
  payees = $state<Payee[]>([]);
  loaded = $state(false);
  error = $state<string | null>(null);

  accountById = $derived(new Map(this.accounts.map((a) => [a.id, a])));
  balanceById = $derived(new Map(this.balances.map((b) => [b.account, b])));
  categoryById = $derived(new Map(this.categories.map((c) => [c.id, c])));
  tagById = $derived(new Map(this.tags.map((t) => [t.id, t])));
  payeeById = $derived(new Map(this.payees.map((p) => [p.id, p])));

  /** An empty book: the shell offers "Load sample data" (D-130). */
  isEmptyBook = $derived(this.loaded && this.accounts.length === 0);

  account(id: AccountId): Account | undefined {
    return this.accountById.get(id);
  }
  balance(id: AccountId): AccountBalance | undefined {
    return this.balanceById.get(id);
  }
  category(id: CategoryId): Category | undefined {
    return this.categoryById.get(id);
  }
  tag(id: TagId): Tag | undefined {
    return this.tagById.get(id);
  }

  payee(id: number): Payee | undefined {
    return this.payeeById.get(id);
  }

  /** "Parent:Child" path, as the register's Category column shows it. */
  categoryPath(id: CategoryId): string {
    const names: string[] = [];
    for (let c = this.categoryById.get(id); c; ) {
      names.unshift(c.name);
      c = c.parent === null ? undefined : this.categoryById.get(c.parent);
    }
    return names.join(":");
  }

  async loadAll(): Promise<void> {
    try {
      const [today, accounts, balances, categories, tags, payees] = await Promise.all([
        call(commands.today()),
        call(commands.accountList()),
        call(commands.accountBalances()),
        call(commands.categoryList()),
        call(commands.tagList()),
        call(commands.payeeList()),
      ]);
      this.today = today;
      this.accounts = accounts;
      this.balances = balances;
      this.categories = categories;
      this.tags = tags;
      this.payees = payees;
      this.error = null;
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
    } finally {
      this.loaded = true;
    }
  }

  async loadCategories(): Promise<void> {
    this.categories = await call(commands.categoryList());
  }

  async loadPayees(): Promise<void> {
    this.payees = await call(commands.payeeList());
  }

  /** After any ledger write: balances change, the lists do not. */
  async loadBalances(): Promise<void> {
    try {
      this.balances = await call(commands.accountBalances());
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
    }
  }
}

export const listsState = new ListsState();
