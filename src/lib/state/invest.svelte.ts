// Securities, and the open investment account's register (INV-030). All
// figures come from Rust as canonical strings; nothing is computed here.

import { call, commands } from "../api";
import type { AccountId, InvRegister, Security, SecurityId } from "../types/bindings";
import { listsState } from "./lists.svelte";

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

class InvestState {
  securities = $state<Security[]>([]);
  securityById = $derived(new Map(this.securities.map((s) => [s.id, s])));

  accountId = $state<AccountId | null>(null);
  register = $state<InvRegister | null>(null);
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

  /** Show an investment account's register. */
  async open(account: AccountId): Promise<void> {
    if (account !== this.accountId) {
      this.accountId = account;
      this.register = null;
    }
    await this.reload();
  }

  async reload(): Promise<void> {
    const account = this.accountId;
    if (account === null) return;
    const seq = ++this.#seq;
    try {
      const [securities, register] = await Promise.all([
        call(commands.securityList()),
        call(commands.invRegister(account)),
      ]);
      if (seq !== this.#seq) return;
      this.securities = securities;
      this.register = register;
      this.error = null;
    } catch (e) {
      if (seq === this.#seq) this.error = message(e);
    }
  }

  /** After a write: this account's register and every account's balance. */
  async refresh(): Promise<void> {
    await Promise.all([this.reload(), listsState.loadBalances()]);
  }
}

export const investState = new InvestState();
