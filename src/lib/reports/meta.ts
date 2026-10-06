// What each report offers in its Customize dialog, and the words for the
// settings' values. Data only: the reports themselves are built in Rust.

import type {
  CompareGroup,
  CompareTo,
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
  /** The comparison reports' "Compare to" and "Subtotal by". */
  compare?: boolean;
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
  compare_category: {
    kind: "compare_category",
    name: "Current Spending vs. Average by Category",
    tabs: ["accounts", "categories", "payees"],
    compare: true,
    totalsOnly: true,
  },
  compare_payee: {
    kind: "compare_payee",
    name: "Current Spending vs. Average by Payee",
    tabs: ["accounts", "categories", "payees"],
    compare: true,
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

/** Every preset's name, offered or not. */
export const PRESETS: [DatePreset, string][] = [
  ["all_dates", "Include all dates"],
  ["monthly", "Monthly"],
  ["quarterly", "Quarterly"],
  ["yearly", "Yearly"],
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
  ["week_to_date", "Week to date"],
  ["last_week", "Last week"],
];

export const presetLabel = (p: DatePreset): string =>
  PRESETS.find(([v]) => v === p)?.[1] ?? p;

/** Presets that pick one month, quarter, or year from a second list. */
export const PERIOD_PRESETS: DatePreset[] = ["monthly", "quarterly", "yearly"];

const labelled = (ps: DatePreset[]): [DatePreset, string][] => ps.map((p) => [p, presetLabel(p)]);

/** The Date range dropdown, in groups shown with a separator between
 * them (RPT-040). Monthly, Quarterly, Yearly: Tax Schedule only. A
 * report saved with a preset no longer offered (This month) gets it as
 * a last group. */
export function presetGroups(kind: ReportKind, current: DatePreset): [DatePreset, string][][] {
  if (REPORTS[kind].compare) {
    const groups: [DatePreset, string][][] = [
      [
        ["week_to_date", "Current week"],
        ["month_to_date", "Current month"],
        ["quarter_to_date", "Current quarter"],
        ["year_to_date", "Current year"],
      ],
      [...labelled(["last_week", "last_month", "last_quarter", "last_year"]), ["custom", "Custom dates"]],
    ];
    if (!groups.some((g) => g.some(([p]) => p === current))) groups.push(labelled([current]));
    return groups;
  }
  const groups = [
    labelled(["all_dates"]),
    ...(kind === "tax_schedule" ? [labelled(PERIOD_PRESETS)] : []),
    labelled(["month_to_date", "quarter_to_date", "year_to_date"]),
    labelled(["last_month", "last_quarter", "last_year", "last_30_days", "last_12_months", "custom"]),
  ];
  if (!groups.some((g) => g.some(([p]) => p === current))) groups.push(labelled([current]));
  return groups;
}

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

export const COMPARES: [CompareTo, string][] = [
  ["weeks_4", "Last 4 weeks"],
  ["weeks_8", "Last 8 weeks"],
  ["weeks_12", "Last 12 weeks"],
  ["months_3", "Last 3 months"],
  ["months_6", "Last 6 months"],
  ["months_12", "Last 12 months"],
  ["quarters_4", "Last 4 quarters"],
  ["quarters_8", "Last 8 quarters"],
  ["quarters_12", "Last 12 quarters"],
  ["years_1", "Last year"],
  ["years_3", "Last 3 years"],
  ["years_5", "Last 5 years"],
];

/** The "Compare to" choices a date range offers (as Rust's
 * `CompareTo::choices`, which also falls back to the first when a
 * saved choice is not offered). */
export function compareChoices(preset: DatePreset): [CompareTo, string][] {
  const unit: Partial<Record<DatePreset, string>> = {
    week_to_date: "weeks",
    last_week: "weeks",
    month_to_date: "months",
    this_month: "months",
    last_month: "months",
    quarter_to_date: "quarters",
    this_quarter: "quarters",
    last_quarter: "quarters",
    year_to_date: "years",
    this_year: "years",
    last_year: "years",
  };
  const prefix = unit[preset] ?? "";
  return COMPARES.filter(([v]) => v.startsWith(prefix));
}

/** `compare` when `preset` offers it, else its first choice. */
export function compareFor(preset: DatePreset, compare: CompareTo | undefined): CompareTo {
  const choices = compareChoices(preset);
  return compare && choices.some(([v]) => v === compare) ? compare : choices[0][0];
}

/** "Subtotal by" for a comparison report: by category offers Payee, by
 * payee offers Category. */
export function compareGroups(kind: ReportKind): [CompareGroup, string][] {
  return [
    ["none", "Don't subtotal"],
    kind === "compare_payee" ? ["category", "Category"] : ["payee", "Payee"],
    ["tag", "Tag"],
    ["account", "Account"],
    ["tax_line", "Tax Schedule"],
  ];
}

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
  "reports.compare_category": "compare_category",
  "reports.compare_payee": "compare_payee",
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
