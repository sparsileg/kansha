// Top-level navigation state (spec §17.3). Plain Svelte + Vite, no
// SvelteKit router: this is the "simple view store" DR-02 calls for.
//
// The views visited form a history stack (each entry a view plus its
// parameters), so Back and Forward can be added to the navigation bar
// without reworking the callers (UI-conventions).

export type ViewId =
  | "dashboard"
  | "account"
  | "window"
  | "manage"
  | "search"
  | "investments"
  | "settings";

export type ManageTab = "payees" | "categories" | "tags" | "securities";

export interface ViewParams {
  /** The Manage view's tab. */
  tab?: ManageTab;
  /** The Search view's text, and the one account it is limited to. */
  q?: string;
  account?: number;
  /** The "window" view: which window (windows.svelte.ts). */
  window?: number;
}

interface Entry {
  view: ViewId;
  params: ViewParams;
}

const MAX_HISTORY = 100;
const same = (a: Entry, view: ViewId, params: ViewParams) =>
  a.view === view && JSON.stringify(a.params) === JSON.stringify(params);

class ViewState {
  entries = $state<Entry[]>([{ view: "dashboard", params: {} }]);
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
    return { view: "dashboard", params: {} };
  }

  /** Drop the entries `drop` matches (a closed window's), and repeats
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
    if (out.length === 0) out.push({ view: "dashboard", params: {} });
    this.entries = out;
    this.index = index;
  }

  /** Forget the history (tests). */
  reset() {
    this.entries = [{ view: "dashboard", params: {} }];
    this.index = 0;
  }

  back() {
    if (this.canBack) this.index -= 1;
  }
  forward() {
    if (this.canForward) this.index += 1;
  }
}

export const viewState = new ViewState();
