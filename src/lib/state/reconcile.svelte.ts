// Reconciliation as the UI shows it (RCN): the account being reconciled,
// its session in progress, or the opening check and history when none is.
// All figures come from the engine; nothing here adds up money.

import { call, commands, withConfirmation } from "../api";
import type {
  AccountId,
  HistoryRow,
  Item,
  OpeningCheck,
  ReconciliationId,
  Session,
  StartInput,
  TxnId,
} from "../types/bindings";
import { confirmState } from "./confirm.svelte";
import { listsState } from "./lists.svelte";
import { registerState } from "./register.svelte";

class ReconcileState {
  accountId = $state<AccountId | null>(null);
  /** The session in progress; `null` when there is none. */
  session = $state<Session | null>(null);
  /** Shown before a statement is entered (RCN-030). */
  opening = $state<OpeningCheck | null>(null);
  history = $state<HistoryRow[]>([]);
  error = $state<string | null>(null);
  busy = $state(false);

  /** Work on this account: load its session or, if none, its opening check
   * and history. */
  async select(id: AccountId | null): Promise<void> {
    this.accountId = id;
    this.session = null;
    this.opening = null;
    this.history = [];
    this.error = null;
    if (id !== null) await this.refresh();
  }

  async refresh(): Promise<void> {
    const id = this.accountId;
    if (id === null) return;
    await this.run(async () => {
      const open = await call(commands.reconcileOpen(id));
      const [session, opening, history] = await Promise.all([
        open ? call(commands.reconcileSession(open.id)) : Promise.resolve(null),
        call(commands.reconcileOpeningCheck(id)),
        call(commands.reconcileHistory(id)),
      ]);
      if (this.accountId !== id) return;
      this.session = session;
      this.opening = opening;
      this.history = history;
    });
  }

  async start(input: StartInput): Promise<boolean> {
    return this.change(async () => {
      await call(commands.reconcileStart(input));
    }, true);
  }

  async check(txns: TxnId[], checked: boolean): Promise<void> {
    const s = this.session;
    if (!s) return;
    await this.change(async () => {
      this.session = await call(commands.reconcileCheck(s.reconciliation.id, txns, checked));
    }, false);
  }

  async update(statementDate: string, statementBalance: string): Promise<boolean> {
    const s = this.session;
    if (!s) return false;
    return this.change(async () => {
      this.session = await call(
        commands.reconcileUpdate(s.reconciliation.id, statementDate, statementBalance),
      );
    }, false);
  }

  /** Balance Adjustment (RCN-040): the engine asks for confirmation. */
  async adjust(): Promise<void> {
    const s = this.session;
    if (!s) return;
    await this.change(async () => {
      const r = await withConfirmation(
        (confirmed) => commands.reconcileAdjust(s.reconciliation.id, confirmed),
        confirmState.ask,
      );
      if (typeof r === "object" && r !== null) this.session = r;
    }, false);
  }

  async finish(): Promise<void> {
    const s = this.session;
    if (!s) return;
    await this.change(async () => {
      await call(commands.reconcileFinish(s.reconciliation.id));
    }, true);
  }

  async abandon(): Promise<void> {
    const s = this.session;
    if (!s) return;
    if (!(await confirmState.ask("Abandon this reconciliation? Items you checked stay checked."))) {
      return;
    }
    await this.change(async () => {
      await call(commands.reconcileAbandon(s.reconciliation.id));
    }, true);
  }

  /** What a finished reconciliation reconciled (RCN-060). */
  async items(id: ReconciliationId): Promise<Item[]> {
    try {
      return await call(commands.reconcileHistoryItems(id));
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
      return [];
    }
  }

  /** Run a change; on success reload what depends on it. `reload`: also
   * refresh the whole view (a session started, finished, or abandoned,
   * or transactions were created). */
  private async change(fn: () => Promise<void>, reload: boolean): Promise<boolean> {
    this.error = null;
    this.busy = true;
    try {
      await fn();
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
      return false;
    } finally {
      this.busy = false;
    }
    if (reload) await this.refresh();
    if (registerState.accountId === this.accountId) await registerState.refresh();
    else await listsState.loadBalances();
    return true;
  }

  private async run(fn: () => Promise<void>): Promise<void> {
    this.busy = true;
    try {
      await fn();
      this.error = null;
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
    } finally {
      this.busy = false;
    }
  }
}

export const reconcileState = new ReconcileState();
