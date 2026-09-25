// Securities, and the open investment account's tabs (POS-040): register,
// holdings, lots, income, performance, realized gains, allocation. All
// figures come from Rust as canonical strings; nothing is computed here.

import { call, commands } from "../api";
import type {
  AccountId,
  Allocation,
  Holdings,
  IncomeReport,
  InvRegister,
  LotView,
  Performance,
  RealizedGain,
  Security,
  SecurityId,
} from "../types/bindings";
import { listsState } from "./lists.svelte";

export type InvTab = "overview" | "transactions" | "holdings" | "lots" | "income" | "performance";

export const INV_TABS: { id: InvTab; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "transactions", label: "Transactions" },
  { id: "holdings", label: "Holdings" },
  { id: "lots", label: "Lots" },
  { id: "income", label: "Income" },
  { id: "performance", label: "Performance" },
];

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

class InvestState {
  securities = $state<Security[]>([]);
  securityById = $derived(new Map(this.securities.map((s) => [s.id, s])));

  accountId = $state<AccountId | null>(null);
  tab = $state<InvTab>("transactions");
  register = $state<InvRegister | null>(null);
  holdings = $state<Holdings | null>(null);
  lots = $state<LotView[]>([]);
  income = $state<IncomeReport | null>(null);
  /** Income tab date range (ISO), empty = open. */
  incomeFrom = $state("");
  incomeTo = $state("");
  performance = $state<Performance | null>(null);
  gains = $state<RealizedGain[]>([]);
  allocation = $state<Allocation | null>(null);
  error = $state<string | null>(null);

  #seq = 0;

  /** Ticker, or name if none. */
  label(id: SecurityId | null): string {
    if (id === null) return "";
    const s = this.securityById.get(id);
    return s ? (s.ticker ?? s.name) : `#${id}`;
  }

  async loadSecurities(): Promise<void> {
    try {
      this.securities = await call(commands.securityList());
    } catch (e) {
      this.error = message(e);
    }
  }

  /** Show an investment account: every tab's figures. */
  async open(account: AccountId): Promise<void> {
    if (account !== this.accountId) {
      this.accountId = account;
      this.register = null;
      this.holdings = null;
      this.lots = [];
      this.income = null;
      this.performance = null;
      this.gains = [];
      this.allocation = null;
      this.incomeFrom = "";
      this.incomeTo = "";
    }
    await this.reload();
  }

  async reload(): Promise<void> {
    const account = this.accountId;
    if (account === null) return;
    const seq = ++this.#seq;
    try {
      const [securities, register, holdings, lots, income, performance, gains, allocation] =
        await Promise.all([
          call(commands.securityList()),
          call(commands.invRegister(account)),
          call(commands.invHoldings(account, null)),
          call(commands.invLots(account, null, null)),
          call(commands.invIncome(account, this.incomeFrom || null, this.incomeTo || null)),
          call(commands.invPerformance(account)),
          call(commands.invGains(account, null, null)),
          call(commands.invAllocation([account])),
        ]);
      if (seq !== this.#seq) return;
      this.securities = securities;
      this.register = register;
      this.holdings = holdings;
      this.lots = lots;
      this.income = income;
      this.performance = performance;
      this.gains = gains;
      this.allocation = allocation;
      this.error = null;
    } catch (e) {
      if (seq === this.#seq) this.error = message(e);
    }
  }

  async setIncomeRange(from: string, to: string): Promise<void> {
    this.incomeFrom = from;
    this.incomeTo = to;
    if (this.accountId === null) return;
    try {
      this.income = await call(commands.invIncome(this.accountId, from || null, to || null));
      this.error = null;
    } catch (e) {
      this.error = message(e);
    }
  }

  /** After a write: this account's tabs and every account's balance. */
  async refresh(): Promise<void> {
    await Promise.all([this.reload(), listsState.loadBalances()]);
  }
}

export const investState = new InvestState();
