// Windows: views that stay open while the user goes elsewhere: reports
// (several at once) and single panels (Calendar, Reminders, Accounts,
// Reconcile). A window fills the view area; showing one is a
// history entry ("window" view), so going anywhere else leaves it in the
// dock, and Back returns to it. The dock lists every open window by name.
// A closed window stays in the history: Back reopens it as it was when
// closed (UI-025).

import { viewState } from "./view.svelte";

export interface Win {
  id: number;
  /** Which component shows it (App.svelte maps kinds to components). */
  kind: string;
  /** Its name, read live (a report's title and dates). */
  label: () => string;
}

/** What a kind of window does when closed. */
export interface WinHooks {
  /** False keeps the window open (e.g. the user cancelled a save prompt). */
  beforeClose?: (id: number) => Promise<boolean>;
  /** The window is gone: drop its state. */
  closed?: (id: number) => void;
  /** What reopening it needs, taken as it closes (a report's settings). */
  keep?: (id: number) => unknown;
  /** A new window, not shown, from what `keep` took. Without it the
   * kind reopens as a plain window of that kind (a panel). */
  reopen?: (kept: unknown) => number;
}

interface Closed {
  kind: string;
  label: string;
  kept: unknown;
}

class WindowState {
  wins = $state<Win[]>([]);
  #hooks = new Map<string, WinHooks>();
  #closed = new Map<number, Closed>();
  #next = 1;

  register(kind: string, hooks: WinHooks): void {
    this.#hooks.set(kind, hooks);
  }

  /** A new window, not yet shown. */
  add(kind: string, label: () => string): number {
    const id = this.#next++;
    this.wins = [...this.wins, { id, kind, label }];
    return id;
  }

  get(id: number): Win | undefined {
    return this.wins.find((w) => w.id === id);
  }

  /** The window showing now, if any. */
  get shown(): number | null {
    return viewState.current === "window" ? (viewState.params.window ?? null) : null;
  }

  /** The kind of the window showing now, if any. */
  get shownKind(): string | null {
    const id = this.shown;
    return id === null ? null : (this.get(id)?.kind ?? null);
  }

  /** Show the one window of a kind (a panel), opening it if needed. */
  openSingle(kind: string, label: string): number {
    const id = this.wins.find((w) => w.kind === kind)?.id ?? this.add(kind, () => label);
    this.show(id);
    return id;
  }

  /** Bring a window to the top. */
  show(id: number): void {
    viewState.navigate("window", { window: id });
  }

  /** Put the showing window back in the dock: the view under it shows. */
  minimize(): void {
    if (this.shown === null) return;
    const base = viewState.base;
    viewState.navigate(base.view, base.params);
  }

  /** Close a window, unless its kind says not to (returns false). */
  async close(id: number): Promise<boolean> {
    const win = this.get(id);
    if (!win) return true;
    const hooks = this.#hooks.get(win.kind);
    if (hooks?.beforeClose && !(await hooks.beforeClose(id))) return false;
    if (this.shown === id) this.minimize();
    this.#closed.set(id, { kind: win.kind, label: win.label(), kept: hooks?.keep?.(id) });
    this.wins = this.wins.filter((w) => w.id !== id);
    hooks?.closed?.(id);
    return true;
  }

  /** The window number `id` was: open, a closed one reopened (its
   * history entries take the new number), or null if it cannot be. A
   * panel already open again is that one. */
  revive(id: number): number | null {
    if (this.get(id)) return id;
    const c = this.#closed.get(id);
    if (!c) return null;
    const hooks = this.#hooks.get(c.kind);
    const label = c.label;
    const fresh = hooks?.reopen
      ? hooks.reopen(c.kept)
      : (this.wins.find((w) => w.kind === c.kind)?.id ?? this.add(c.kind, () => label));
    this.#closed.delete(id);
    viewState.renumber(id, fresh);
    return fresh;
  }

  /** The window is open, or closed and can be reopened. */
  canRevive(id: number): boolean {
    return this.get(id) !== undefined || this.#closed.has(id);
  }

  /** A window's name, open or closed (Back's tooltip). */
  nameOf(id: number): string | null {
    return this.labels().get(id) ?? this.#closed.get(id)?.label ?? null;
  }

  /** Before quitting (File > Exit): ask each window's kind whether it may
   * close (a changed report asks to save). False as soon as one says no;
   * nothing is closed either way. */
  async mayQuit(): Promise<boolean> {
    for (const w of [...this.wins]) {
      const hook = this.#hooks.get(w.kind)?.beforeClose;
      if (hook && !(await hook(w.id))) return false;
    }
    return true;
  }

  /** Dock names: a repeated name gets a number from its second copy on
   * ("Net Worth (2)"). */
  labels(): Map<number, string> {
    const out = new Map<number, string>();
    const count = new Map<string, number>();
    for (const w of this.wins) {
      const name = w.label();
      const n = (count.get(name) ?? 0) + 1;
      count.set(name, n);
      out.set(w.id, n === 1 ? name : `${name} (${n})`);
    }
    return out;
  }

  /** Close everything without asking (tests). */
  reset(): void {
    for (const w of this.wins) this.#hooks.get(w.kind)?.closed?.(w.id);
    this.wins = [];
    this.#closed.clear();
  }
}

export const windowState = new WindowState();
