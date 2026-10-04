// What each report offers in its Customize dialog, and the words for the
// settings' values. Data only: the reports themselves are built in Rust.

import type {
  DatePreset,
  DetailSort,
  Interval,
  ReportKind,
  Subtotal,
  TaxGroup,
} from "../types/bindings";

export type FilterTab = "accounts" | "categories" | "payees" | "securities" | "tags";

export interface ReportMeta {
  kind: ReportKind;
  /** Menu and dialog name. */
  name: string;
  /** Filter tabs after Display. */
  tabs: FilterTab[];
  subtotal?: boolean;
  /** Tax Summary's own "Subtotal by". */
  taxGroup?: boolean;
  interval?: boolean;
  sort?: boolean;
  totalsOnly?: boolean;
  transfers?: boolean;
  showZero?: boolean;
}

const SPENDING: FilterTab[] = ["accounts", "categories", "payees", "tags"];

export const REPORTS: Record<ReportKind, ReportMeta> = {
  capital_gains: {
    kind: "capital_gains",
    name: "Capital Gains",
    tabs: ["accounts", "securities"],
    subtotal: true,
    totalsOnly: true,
  },
  net_worth: {
    kind: "net_worth",
    name: "Net Worth",
    tabs: ["accounts"],
    interval: true,
    totalsOnly: true,
    showZero: true,
  },
  itemized_categories: {
    kind: "itemized_categories",
    name: "Itemized Categories",
    tabs: SPENDING,
    sort: true,
    totalsOnly: true,
    transfers: true,
  },
  itemized_payees: {
    kind: "itemized_payees",
    name: "Itemized Payees",
    tabs: SPENDING,
    sort: true,
    totalsOnly: true,
    transfers: true,
  },
  income_expense: {
    kind: "income_expense",
    name: "Income/Expense by Category",
    tabs: SPENDING,
    interval: true,
    totalsOnly: true,
  },
  income_expense_payee: {
    kind: "income_expense_payee",
    name: "Income/Expense by Payee",
    tabs: SPENDING,
    interval: true,
    totalsOnly: true,
  },
  performance: {
    kind: "performance",
    name: "Investment Performance",
    tabs: ["accounts"],
    totalsOnly: true,
  },
  investment_income: {
    kind: "investment_income",
    name: "Investment Income",
    tabs: ["accounts", "securities"],
    totalsOnly: true,
  },
  holdings: {
    kind: "holdings",
    name: "Holdings",
    tabs: ["accounts", "securities"],
    totalsOnly: true,
  },
  asset_allocation: {
    kind: "asset_allocation",
    name: "Asset Allocation",
    tabs: ["accounts"],
  },
  tax_schedule: {
    kind: "tax_schedule",
    name: "Tax Schedule",
    tabs: ["accounts", "categories", "payees", "securities"],
    sort: true,
    totalsOnly: true,
  },
  tax_summary: {
    kind: "tax_summary",
    name: "Tax Summary",
    tabs: SPENDING,
    taxGroup: true,
    sort: true,
    totalsOnly: true,
  },
};

export const TAB_LABELS: Record<FilterTab, string> = {
  accounts: "Accounts",
  categories: "Categories",
  payees: "Payees",
  securities: "Securities",
  tags: "Tags",
};

export const PRESETS: [DatePreset, string][] = [
  ["all_dates", "Include all dates"],
  ["month_to_date", "Month to date"],
  ["quarter_to_date", "Quarter to date"],
  ["year_to_date", "Year to date"],
  ["this_month", "This month"],
  ["last_month", "Last month"],
  ["this_quarter", "This quarter"],
  ["last_quarter", "Last quarter"],
  ["this_year", "This year"],
  ["last_year", "Last year"],
  ["last_30_days", "Last 30 days"],
  ["last_12_months", "Last 12 months"],
  ["custom", "Custom dates"],
];

export const presetLabel = (p: DatePreset): string =>
  PRESETS.find(([v]) => v === p)?.[1] ?? p;

export const INTERVALS: [Interval, string][] = [
  ["none", "None"],
  ["week", "Week"],
  ["two_weeks", "Two weeks"],
  ["half_month", "Half month"],
  ["month", "Month"],
  ["quarter", "Quarter"],
  ["half_year", "Half year"],
  ["year", "Year"],
];

export const SUBTOTALS: [Subtotal, string][] = [
  ["term", "Short vs. long-term"],
  ["month", "Month"],
  ["quarter", "Quarter"],
  ["year", "Year"],
  ["account", "Account"],
  ["security", "Security"],
  ["none", "Don't subtotal"],
];

export const TAX_GROUPS: [TaxGroup, string][] = [
  ["category", "Category"],
  ["tax_line", "Tax line"],
  ["account", "Account"],
  ["payee", "Payee"],
  ["tag", "Tag"],
  ["month", "Month"],
  ["quarter", "Quarter"],
  ["year", "Year"],
  ["none", "Don't subtotal"],
];

export const SORTS: [DetailSort, string][] = [
  ["date", "Date/Account"],
  ["account_date", "Account/Date"],
  ["amount", "Amount"],
];

/** Column headings that sort an itemized report when clicked. */
export const COLUMN_SORTS: Record<string, DetailSort> = {
  date: "date",
  account: "account_date",
  num: "num",
};

/** Reports with a "Sort by" dropdown and sortable column headings. */
export const SORTABLE: ReportKind[] = ["itemized_categories", "itemized_payees", "tax_summary"];

/** The Reports menu: item id → report kind. Capital Gains is listed under
 * Investing and again under Tax. */
export const MENU_REPORTS: Record<string, ReportKind> = {
  "reports.capital_gains": "capital_gains",
  "reports.performance": "performance",
  "reports.investment_income": "investment_income",
  "reports.holdings": "holdings",
  "reports.asset_allocation": "asset_allocation",
  "reports.net_worth": "net_worth",
  "reports.itemized_categories": "itemized_categories",
  "reports.itemized_payees": "itemized_payees",
  "reports.income_expense": "income_expense",
  "reports.income_expense_payee": "income_expense_payee",
  "reports.tax_capital_gains": "capital_gains",
  "reports.tax_schedule": "tax_schedule",
  "reports.tax_summary": "tax_summary",
};

/** Add or remove one id from a filter. `null` stands for `base` (what
 * the report includes with no filter); when `base` is everything, a
 * choice of every id becomes `null` again, so new accounts or categories
 * are included later too. */
export function toggleFilter<T>(
  filter: T[] | null,
  base: T[],
  all: T[],
  id: T,
  on: boolean,
): T[] | null {
  const current = filter ?? base;
  const next = on ? (current.includes(id) ? current : [...current, id]) : current.filter((x) => x !== id);
  const baseIsAll = all.every((x) => base.includes(x));
  return baseIsAll && all.every((x) => next.includes(x)) ? null : next;
}
