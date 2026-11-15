// Reconciliation as the UI shows it (RCN): the account being reconciled,
// its session in progress, or the opening check and history when none is.
// All figures come from the engine; nothing here adds up money.

import { call, commands, withConfirmation } from "../api";
import { isReconcilable } from "../reconcile/form";
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
  /** Reconcilable accounts with a session in progress, marked in the
   * account picker. */
  inProgress = $state<AccountId[]>([]);
  error = $state<string | null>(null);
  busy = $state(false);
  /** Checks being saved, one after another. */
  #checks: Promise<void> = Promise.resolve();
  #queued = 0;

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
      const others = listsState.accounts
        .filter((a) => a.status === "open" && isReconcilable(a))
        .map((a) => a.id);
      const [open, othersOpen] = await Promise.all([
        call(commands.reconcileOpen(id)),
        Promise.all(others.map((a) => call(commands.reconcileOpen(a)))),
      ]);
      const [session, opening, history] = await Promise.all([
        open ? call(commands.reconcileSession(open.id)) : Promise.resolve(null),
        call(commands.reconcileOpeningCheck(id)),
        call(commands.reconcileHistory(id)),
      ]);
      if (this.accountId !== id) return;
      this.session = session;
      this.opening = opening;
      this.history = history;
      this.inProgress = others.filter((_, i) => othersOpen[i] !== null);
    });
  }

  async start(input: StartInput): Promise<boolean> {
    return this.change(async () => {
      await call(commands.reconcileStart(input));
    }, true);
  }

  /** Show items checked or not at once (RCN-080), before the engine
   * saves it; `check` saves it. */
  mark(txns: TxnId[], checked: boolean): void {
    const s = this.session;
    if (!s) return;
    const ids = new Set(txns);
    for (const i of [...s.payments, ...s.deposits]) if (ids.has(i.txn_id)) i.checked = checked;
  }

  /** Check or uncheck items: marked at once, saved in the order asked.
   * Checking moves no balance, so the account bar is not read again,
   * and an open register of the account reads them when next shown. */
  check(txns: TxnId[], checked: boolean): Promise<void> {
    const s = this.session;
    if (!s) return Promise.resolve();
    const id = s.reconciliation.id;
    this.mark(txns, checked);
    if (registerState.accountId === this.accountId) registerState.stale = true;
    this.#queued++;
    const run = this.#checks.then(async () => {
      try {
        const next = await call(commands.reconcileCheck(id, txns, checked));
        // A check still waiting brings the session; this one would undo
        // its mark until then.
        if (this.#queued === 1 && this.session?.reconciliation.id === id) this.session = next;
        this.error = null;
      } catch (e) {
        // Back to what is saved, then say why.
        if (this.#queued === 1) await this.refresh();
        this.error = e instanceof Error ? e.message : String(e);
      } finally {
        this.#queued--;
      }
    });
    this.#checks = run;
    return run;
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
