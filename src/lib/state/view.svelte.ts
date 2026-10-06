// Top-level navigation state (spec §17.3). Plain Svelte + Vite, no
// SvelteKit router: this is the "simple view store" DR-02 calls for.
//
// The views visited form a history stack (each entry a view plus its
// parameters), walked by the navigation bar's Back and Forward arrows
// (UI-025; `shell/history.ts` reopens what an entry needs).

export type ViewId =
  | "insights"
  | "account"
  | "window"
  | "manage"
  | "search"
  | "settings";

export type ManageTab = "payees" | "categories" | "tags" | "securities";

export interface ViewParams {
  /** The Manage view's tab. */
  tab?: ManageTab;
  /** The Search view's text, and the one account it is limited to;
   * the "account" view: whose register. */
  q?: string;
  account?: number;
  /** The "window" view: which window (windows.svelte.ts). */
  window?: number;
  /** The Insights view ("insights"): which insight's tab; the first
   * when absent. */
  insight?: number;
}

export interface Entry {
  view: ViewId;
  params: ViewParams;
}

const MAX_HISTORY = 100;
const same = (a: Entry, view: ViewId, params: ViewParams) =>
  a.view === view && JSON.stringify(a.params) === JSON.stringify(params);

class ViewState {
  entries = $state<Entry[]>([{ view: "insights", params: {} }]);
  index = $state(0);

  get current(): ViewId {
    return this.entries[this.index].view;
  }
  get params(): ViewParams {
    return this.entries[this.index].params;
  }
  /** Nothing visited yet beyond the starting view. */
  get untouched(): boolean {
    return this.entries.length === 1;
  }
  get canBack(): boolean {
    return this.index > 0;
  }
  get canForward(): boolean {
    return this.index < this.entries.length - 1;
  }

  /** Go to a view. Going where you already are adds nothing; going
   * somewhere new drops any forward entries. */
  navigate(view: ViewId, params: ViewParams = {}) {
    if (same(this.entries[this.index], view, params)) return;
    const kept = [...this.entries.slice(0, this.index + 1), { view, params }];
    this.entries = kept.slice(-MAX_HISTORY);
    this.index = this.entries.length - 1;
  }

  /** The nearest entry up to the current one that is not a window: what
   * shows under the windows. */
  get base(): Entry {
    for (let i = this.index; i >= 0; i--) {
      if (this.entries[i].view !== "window") return this.entries[i];
    }
    return { view: "insights", params: {} };
  }

  /** The entry `delta` steps away (-1 Back, +1 Forward), if any. */
  peek(delta: number): Entry | null {
    return this.entries[this.index + delta] ?? null;
  }

  /** A closed window came back with a new number: its entries follow. */
  renumber(from: number, to: number) {
    this.entries = this.entries.map((e) =>
      e.view === "window" && e.params.window === from ? { ...e, params: { ...e.params, window: to } } : e,
    );
  }

  /** Drop the entries `drop` matches (an account deleted), and repeats
   * that leaves side by side. The current entry stays current when kept;
   * otherwise the one before it becomes current. */
  forget(drop: (e: Entry) => boolean) {
    const out: Entry[] = [];
    let index = 0;
    this.entries.forEach((e, i) => {
      const last = out[out.length - 1];
      if (!drop(e) && !(last && same(last, e.view, e.params))) out.push(e);
      if (i <= this.index) index = Math.max(out.length - 1, 0);
    });
    if (out.length === 0) out.push({ view: "insights", params: {} });
    this.entries = out;
    this.index = index;
  }

  /** Forget what came before the current entry: the history starts
   * here (the startup screen). */
  startHere() {
    this.entries = [this.entries[this.index]];
    this.index = 0;
  }

  /** Forget the history (tests). */
  reset() {
    this.entries = [{ view: "insights", params: {} }];
    this.index = 0;
  }

  back() {
    if (this.canBack) this.index -= 1;
  }
  /** Leave the Search view for the view the search was made from: the
   * nearest earlier entry that is not a search, as Back to it would. */
  leaveSearch() {
    for (let i = this.index - 1; i >= 0; i--) {
      if (this.entries[i].view !== "search") {
        this.index = i;
        return;
      }
    }
    this.navigate("insights");
  }
  forward() {
    if (this.canForward) this.index += 1;
  }
}

export const viewState = new ViewState();
