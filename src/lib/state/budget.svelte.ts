// Budgets (BUD-020, BUD-040): the list for the Budget menu, and one
// window per open budget. A window keeps what it shows (view, year,
// Show, period) so the dock and Back bring it back as it was.

import { call, commands } from "../api";
import type { Budget, BudgetId, GraphAt, GraphPeriod } from "../types/bindings";
import { windowState } from "./windows.svelte";

export const BUDGET_WINDOW = "budget";

export type BudgetShow = "details" | "budget" | "balance" | "actual";

export interface BudgetWin {
  budget: BudgetId;
  view: "annual" | "graph";
  /** The Annual View's year. */
  year: number;
  show: BudgetShow;
  period: GraphPeriod;
  /** The Graph View's month, quarter, or year; `null` the current one. */
  at: GraphAt | null;
}

class BudgetState {
  /** Every budget, by name. */
  list = $state<Budget[]>([]);
  /** The name dialog: create, copy, or rename. */
  naming = $state<{ mode: "create" | "copy" | "rename"; id: BudgetId | null; name: string } | null>(null);
  /** Bumped when a budget's categories change: open windows reload. */
  version = $state(0);
  /** Select categories to budget… for this budget. */
  choosing = $state<BudgetId | null>(null);
  /** Each budget window's state, by window ID (deeply reactive). */
  #wins = $state<Record<number, BudgetWin>>({});

  constructor() {
    windowState.register(BUDGET_WINDOW, {
      closed: (id) => delete this.#wins[id],
      keep: (id) => {
        const w = this.#wins[id];
        return w && $state.snapshot(w);
      },
      reopen: (kept) => this.#add(kept as BudgetWin),
    });
  }

  async refresh(): Promise<void> {
    this.list = await call(commands.budgetList());
  }

  name(id: BudgetId): string {
    return this.list.find((b) => b.id === id)?.name ?? "Budget";
  }

  win(id: number): BudgetWin | undefined {
    return this.#wins[id];
  }

  #add(w: BudgetWin): number {
    const id = windowState.add(BUDGET_WINDOW, () => this.name(w.budget));
    this.#wins[id] = w;
    return id;
  }

  /** Show a budget: its open window, else a new one on this year. */
  open(budget: BudgetId, year: number): void {
    const open = Object.entries(this.#wins).find(([, w]) => w.budget === budget);
    if (open) {
      windowState.show(Number(open[0]));
      return;
    }
    windowState.show(
      this.#add({ budget, view: "annual", year, show: "details", period: "monthly", at: null }),
    );
  }

  /** A budget was deleted: close its windows. */
  async closeBudget(budget: BudgetId): Promise<void> {
    for (const [id, w] of Object.entries(this.#wins)) {
      if (w.budget === budget) await windowState.close(Number(id));
    }
  }
}

export const budgetState = new BudgetState();
