// Non-visual UI settings. Theme and font size live in theme.svelte.ts.
// Runtime-only until the `settings` module persists them (SET-070).

class SettingsState {
  /** Register page size (NFR-040). */
  pageSize = $state(100);
  /** Account selector: top dropdown or persistent sidebar (UI-010). */
  accountNav = $state<"dropdown" | "sidebar">("dropdown");
  showClosedAccounts = $state(false);

  toggleAccountNav() {
    this.accountNav = this.accountNav === "dropdown" ? "sidebar" : "dropdown";
  }
}

export const settingsState = new SettingsState();
