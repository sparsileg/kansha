// The menu bar's contents (UI-conventions). Data only: App.svelte maps an
// item's `id` to what it does. An item with `disabled` is shown greyed out
// with that text as its reason, so what is still to come stays in view.

export interface MenuItem {
  id: string;
  label: string;
  /** Why it is unavailable; present means greyed out. */
  disabled?: string;
  /** A separator line above this item. */
  divider?: boolean;
}

export interface Menu {
  id: string;
  label: string;
  items: MenuItem[];
}

const later = (phase: string) => `Planned: ${phase}`;

export const MENUS: Menu[] = [
  {
    id: "file",
    label: "File",
    items: [
      { id: "file.new", label: "New…", disabled: later("separate books") },
      { id: "file.open", label: "Open…", disabled: later("separate books") },
      { id: "file.backup", label: "Backup…", disabled: later("Phase 8"), divider: true },
      { id: "file.restore", label: "Restore…", disabled: later("Phase 8") },
      { id: "file.import", label: "Import…", disabled: later("import"), divider: true },
      { id: "file.export", label: "Export…", disabled: later("export") },
      { id: "file.integrity", label: "Integrity Check", divider: true },
      { id: "file.exit", label: "Exit", divider: true },
    ],
  },
  {
    id: "edit",
    label: "Edit",
    items: [
      { id: "edit.settings", label: "Settings…" },
      { id: "edit.navbar", label: "Navigation Bar…" },
      { id: "edit.renaming", label: "Renaming…", disabled: later("to be defined") },
    ],
  },
  {
    id: "tools",
    label: "Tools",
    items: [
      { id: "tools.accounts", label: "Accounts" },
      { id: "tools.calendar", label: "Calendar" },
      { id: "tools.reminders", label: "Reminders" },
      { id: "tools.payees", label: "Payees", divider: true },
      { id: "tools.categories", label: "Categories" },
      { id: "tools.tags", label: "Tags" },
      { id: "tools.securities", label: "Securities" },
      { id: "tools.reconcile", label: "Reconcile", divider: true },
    ],
  },
  {
    id: "reports",
    label: "Reports",
    items: [
      { id: "reports.saved", label: "Saved", disabled: later("Phase 7") },
      { id: "reports.investing", label: "Investing", disabled: later("Phase 7") },
      { id: "reports.balances", label: "Balances", disabled: later("Phase 7") },
      { id: "reports.spending", label: "Spending", disabled: later("Phase 7") },
      { id: "reports.taxes", label: "Taxes", disabled: later("Phase 7") },
    ],
  },
  {
    id: "help",
    label: "Help",
    items: [{ id: "help.about", label: "About Kansha", disabled: later("Phase 8") }],
  },
];
