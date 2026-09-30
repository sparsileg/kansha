// The Investments screen: its named views, the as-of date, and the
// overview Rust works out (POS-010, LOT-150). Views are kept in the book
// (SET-070) through booksettings.svelte.ts; `applyStored` reads them when
// a book opens. Whether closed lots show is part of each view. Which rows
// are expanded is kept while the app runs.

import { call, commands } from "../api";
import {
  defaultView,
  parseViews,
  serializeViews,
  shownAccounts,
  shownSecurities,
  type ViewDef,
  type ViewsState,
} from "../invest/views";
import type { Portfolio } from "../types/bindings";
import { listsState } from "./lists.svelte";
import { investState } from "./invest.svelte";
import { bookSettings } from "./booksettings.svelte";

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

class InvestViewState {
  #stored = parseViews(bookSettings.value.invest_views ?? "");
  views = $state<ViewDef[]>(this.#stored.views);
  selected = $state(this.#stored.selected);
  /** The valuation date (ISO). */
  asOf = $state("");
  portfolio = $state<Portfolio | null>(null);
  error = $state<string | null>(null);
  /** Keys of expanded rows: "a<account>" and "p<account>:<security>". */
  expanded = $state<Set<string>>(new Set());

  #seq = 0;

  view = $derived(this.views[this.selected]);
  /** Show each lot's sales and the securities sold out (POS-040); kept
   * per view. */
  showClosed = $derived(this.view.showClosed);
  /** Open investment accounts, in the account list's order. */
  available = $derived(
    listsState.accounts.filter((a) => a.investment && a.status === "open").map((a) => a.id),
  );

  #save() {
    const s: ViewsState = { views: this.views, selected: this.selected };
    void bookSettings.update({ invest_views: serializeViews(s) });
  }

  /** Take the views stored in the book (after it opens). */
  applyStored() {
    const s = parseViews(bookSettings.value.invest_views ?? "");
    this.views = s.views;
    this.selected = s.selected;
  }

  select(slot: number) {
    if (slot < 0 || slot >= this.views.length) return;
    this.selected = slot;
    this.#save();
    void this.load();
  }

  /** Replace the selected view (Customize > OK). */
  update(view: ViewDef) {
    this.views = this.views.map((v, i) => (i === this.selected ? view : v));
    this.#save();
    void this.load();
  }

  /** The selected slot's original view, for Reset View. */
  fresh(): ViewDef {
    return defaultView(this.selected);
  }

  setShowClosed(on: boolean) {
    this.update({ ...this.view, showClosed: on });
  }

  toggle(key: string) {
    const next = new Set(this.expanded);
    if (!next.delete(key)) next.add(key);
    this.expanded = next;
  }

  /** Load the overview for the selected view and date. */
  async load(): Promise<void> {
    if (this.asOf === "") this.asOf = listsState.today;
    const seq = ++this.#seq;
    try {
      const accounts = shownAccounts(this.view, this.available);
      if (investState.securities.length === 0) await investState.loadSecurities();
      const securities = shownSecurities(
        this.view,
        investState.securities.map((s) => s.id),
      );
      const p = await call(commands.invPortfolio(accounts, securities, this.asOf || null, this.showClosed));
      if (seq !== this.#seq) return;
      this.portfolio = p;
      this.error = null;
    } catch (e) {
      if (seq === this.#seq) this.error = message(e);
    }
  }
}

export const investViewState = new InvestViewState();
