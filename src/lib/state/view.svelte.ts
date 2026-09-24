// Top-level navigation state (spec §17.3). Plain Svelte + Vite, no
// SvelteKit router: this is the "simple view store" DR-02 calls for.

export type ViewId =
  | "dashboard"
  | "account"
  | "scheduled"
  | "calendar"
  | "reconcile"
  | "reports"
  | "manage"
  | "settings";

class ViewState {
  current = $state<ViewId>("dashboard");

  navigate(view: ViewId) {
    this.current = view;
  }
}

export const viewState = new ViewState();
