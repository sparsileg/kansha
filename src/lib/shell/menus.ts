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
  /** A submenu: the item opens these instead of running. */
  items?: MenuItem[];
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
      { id: "file.new", label: "New…" },
      { id: "file.open", label: "Open…" },
      // Filled from the recent books list (App.svelte).
      { id: "file.recent", label: "Recent", items: [] },
      { id: "file.rename", label: "Rename Book…" },
      { id: "file.backup", label: "Back Up Now", divider: true },
      { id: "file.restore", label: "Restore…" },
      { id: "file.import", label: "Import…", divider: true },
      { id: "file.export", label: "Export…", disabled: later("export") },
      { id: "file.integrity", label: "Integrity Check", divider: true },
      { id: "file.exit", label: "Exit", divider: true },
    ],
  },
  {
    id: "edit",
    label: "Edit",
    items: [
      { id: "edit.undo", label: "Undo (Ctrl+Z)" },
      { id: "edit.settings", label: "Settings…", divider: true },
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
      { id: "tools.payees", label: "Memorized Payees", divider: true },
      { id: "tools.categories", label: "Categories" },
      { id: "tools.tags", label: "Tags" },
      { id: "tools.securities", label: "Securities" },
      { id: "tools.import_prices", label: "Import Prices…" },
      { id: "tools.reconcile", label: "Reconcile", divider: true },
    ],
  },
  {
    id: "reports",
    label: "Reports",
    items: [
      { id: "reports.saved", label: "Saved Reports…" },
      {
        id: "reports.investing",
        label: "Investing",
        divider: true,
        items: [
          { id: "reports.capital_gains", label: "Capital Gains" },
          { id: "reports.performance", label: "Investment Performance" },
          { id: "reports.investment_income", label: "Investment Income" },
          { id: "reports.holdings", label: "Holdings" },
          { id: "reports.asset_allocation", label: "Asset Allocation" },
        ],
      },
      { id: "reports.networth", label: "Net Worth", items: [{ id: "reports.net_worth", label: "Net Worth" }] },
      {
        id: "reports.spending",
        label: "Spending",
        items: [
          { id: "reports.itemized_categories", label: "Itemized Categories" },
          { id: "reports.itemized_payees", label: "Itemized Payees" },
          { id: "reports.income_expense", label: "Income/Expense by Category" },
          { id: "reports.income_expense_payee", label: "Income/Expense by Payee" },
        ],
      },
      {
        id: "reports.tax",
        label: "Tax",
        items: [
          { id: "reports.tax_capital_gains", label: "Capital Gains" },
          { id: "reports.tax_schedule", label: "Tax Schedule" },
          { id: "reports.tax_summary", label: "Tax Summary" },
        ],
      },
    ],
  },
  {
    id: "help",
    label: "Help",
    items: [{ id: "help.about", label: "About Kansha" }],
  },
];

/** Every item that runs something (submenu items, not the submenus). */
export function leafItems(items: MenuItem[]): MenuItem[] {
  return items.flatMap((i) => (i.items ? leafItems(i.items) : [i]));
}
