// Reference data the whole UI reads: today, accounts and their balances,
// categories, tags. Loaded from IPC; all figures stay canonical strings.

import { call, commands } from "../api";
import type {
  Account,
  AccountBalance,
  AccountId,
  Category,
  CategoryId,
  Insight,
  SpendingCard,
  Payee,
  Tag,
  TagId,
  TaxLine,
  TaxLineId,
} from "../types/bindings";

class ListsState {
  /** The Rust clock's today (ISO); empty until `loadAll` finishes. */
  today = $state("");
  accounts = $state<Account[]>([]);
  balances = $state<AccountBalance[]>([]);
  /** Net worth today, from Rust, for the foot of the account list;
   * `null` until loaded or when it could not be. */
  netWorth = $state<string | null>(null);
  /** Each account list section's total, from Rust, by section id. */
  sectionTotals = $state<Record<string, string>>({});
  categories = $state<Category[]>([]);
  tags = $state<Tag[]>([]);
  payees = $state<Payee[]>([]);
  /** Tax form lines (CAT-050): built in, loaded once. */
  taxLines = $state<TaxLine[]>([]);
  /** Insights (INS-010), in tab order. */
  insights = $state<Insight[]>([]);
  /** Spending cards (CARD-060), by name. */
  spendingCards = $state<SpendingCard[]>([]);
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

  /** "Form: Line" for a tax line. */
  taxLineLabel(id: TaxLineId): string {
    const t = this.taxLines.find((x) => x.id === id);
    return t ? `${t.form}: ${t.line}` : "";
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
      const [today, accounts, balances, categories, tags, payees, taxLines, insights, spendingCards] = await Promise.all([
        call(commands.today()),
        call(commands.accountList()),
        call(commands.accountBalances()),
        call(commands.categoryList()),
        call(commands.tagList()),
        call(commands.payeeList()),
        call(commands.taxLineList()),
        call(commands.insightList()),
        call(commands.spendingCardList()),
      ]);
      this.today = today;
      this.accounts = accounts;
      this.balances = balances;
      this.categories = categories;
      this.tags = tags;
      this.payees = payees;
      this.taxLines = taxLines;
      this.insights = insights;
      this.spendingCards = spendingCards;
      this.error = null;
      void this.loadNetWorth();
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
    } finally {
      this.loaded = true;
    }
  }

  async loadCategories(): Promise<void> {
    this.categories = await call(commands.categoryList());
  }

  /** The insights and the spending cards they can show. */
  async loadInsights(): Promise<void> {
    [this.insights, this.spendingCards] = await Promise.all([call(commands.insightList()), call(commands.spendingCardList())]);
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
    await this.loadNetWorth();
    await this.loadSectionTotals();
  }

  /** A failure only drops the totals: the list still works. */
  async loadSectionTotals(): Promise<void> {
    try {
      const rows = await call(commands.sectionTotals());
      this.sectionTotals = Object.fromEntries(rows.map((r) => [r.section, r.total]));
    } catch {
      this.sectionTotals = {};
    }
  }

  /** Net worth changes with balances and prices. A failure only blanks
   * the figure: the rest of the list still works. */
  async loadNetWorth(): Promise<void> {
    try {
      this.netWorth = await call(commands.netWorth());
    } catch {
      this.netWorth = null;
    }
  }
}

export const listsState = new ListsState();
