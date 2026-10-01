// Named views of the Investments screen (the "View:" list): which columns
// show and in what order, which accounts and in what order, which
// securities. Pure logic; storage is in state/investview.svelte.ts.

export type ColumnId =
  | "ticker"
  | "price"
  | "shares"
  | "cost_basis"
  | "market_value"
  | "gain"
  | "day_gain"
  | "day_percent";

/** Every column but Name, which always shows. */
export const COLUMNS: { id: ColumnId; label: string }[] = [
  { id: "ticker", label: "Ticker Symbol" },
  { id: "price", label: "Quote/Price" },
  { id: "shares", label: "Shares" },
  { id: "cost_basis", label: "Cost Basis" },
  { id: "market_value", label: "Market Value" },
  { id: "gain", label: "Gain/Loss" },
  { id: "day_gain", label: "Day Gain/Loss" },
  { id: "day_percent", label: "Price Day Change (%)" },
];

export const columnLabel = (id: ColumnId): string => COLUMNS.find((c) => c.id === id)?.label ?? id;

export const DEFAULT_COLUMNS: ColumnId[] = COLUMNS.map((c) => c.id).filter((id) => id !== "cost_basis");

export const VIEW_COUNT = 5;

export interface ViewDef {
  name: string;
  /** Shown columns, left to right. */
  columns: ColumnId[];
  /** Account order (ids); accounts not listed follow in their own order. */
  accountOrder: number[];
  hiddenAccounts: number[];
  hiddenSecurities: number[];
  /** Show each lot's sales and the securities sold out (POS-040). */
  showClosed: boolean;
}

export interface ViewsState {
  views: ViewDef[];
  selected: number;
}

export const defaultName = (slot: number): string => (slot === 0 ? "Default" : `Custom ${slot + 1}`);

export function defaultView(slot: number): ViewDef {
  return {
    name: defaultName(slot),
    columns: [...DEFAULT_COLUMNS],
    accountOrder: [],
    hiddenAccounts: [],
    hiddenSecurities: [],
    showClosed: false,
  };
}

export function defaultViews(): ViewsState {
  return { views: Array.from({ length: VIEW_COUNT }, (_, i) => defaultView(i)), selected: 0 };
}

const isIds = (v: unknown): v is number[] => Array.isArray(v) && v.every((n) => Number.isInteger(n));

function cleanView(v: unknown, slot: number): ViewDef {
  const base = defaultView(slot);
  if (typeof v !== "object" || v === null) return base;
  const o = v as Record<string, unknown>;
  const known = new Set<string>(COLUMNS.map((c) => c.id));
  const columns = Array.isArray(o.columns)
    ? [...new Set(o.columns.filter((c): c is ColumnId => typeof c === "string" && known.has(c)))]
    : base.columns;
  return {
    name: typeof o.name === "string" && o.name.trim() ? o.name.trim() : base.name,
    columns,
    accountOrder: isIds(o.accountOrder) ? o.accountOrder : [],
    hiddenAccounts: isIds(o.hiddenAccounts) ? o.hiddenAccounts : [],
    hiddenSecurities: isIds(o.hiddenSecurities) ? o.hiddenSecurities : [],
    showClosed: o.showClosed === true,
  };
}

/** Stored text back into views; anything unreadable gives the defaults. */
export function parseViews(text: string): ViewsState {
  try {
    const raw: unknown = JSON.parse(text);
    if (typeof raw !== "object" || raw === null) return defaultViews();
    const o = raw as Record<string, unknown>;
    const list = Array.isArray(o.views) ? o.views : [];
    const views = Array.from({ length: VIEW_COUNT }, (_, i) => cleanView(list[i], i));
    const sel = typeof o.selected === "number" && Number.isInteger(o.selected) ? o.selected : 0;
    return { views, selected: sel >= 0 && sel < VIEW_COUNT ? sel : 0 };
  } catch {
    return defaultViews();
  }
}

export const serializeViews = (s: ViewsState): string => JSON.stringify(s);

/** Accounts to show: `available` (open investment accounts) in the view's
 * order, new ones last, minus the hidden. */
export function shownAccounts(view: ViewDef, available: number[]): number[] {
  const have = new Set(available);
  const ordered = view.accountOrder.filter((id) => have.has(id));
  const placed = new Set(ordered);
  const all = [...ordered, ...available.filter((id) => !placed.has(id))];
  return all.filter((id) => !view.hiddenAccounts.includes(id));
}

/** Every account in the view's order, hidden ones too (the Accounts tab). */
export function orderedAccounts(view: ViewDef, available: number[]): number[] {
  const have = new Set(available);
  const ordered = view.accountOrder.filter((id) => have.has(id));
  const placed = new Set(ordered);
  return [...ordered, ...available.filter((id) => !placed.has(id))];
}

/** Securities to leave out, as the ids to pass to Rust: everything not
 * hidden, or `null` when nothing is hidden. */
export function shownSecurities(view: ViewDef, all: number[]): number[] | null {
  return view.hiddenSecurities.length === 0 ? null : all.filter((id) => !view.hiddenSecurities.includes(id));
}

/** Columns not shown, in the canonical order. */
export function availableColumns(shown: ColumnId[]): ColumnId[] {
  return COLUMNS.map((c) => c.id).filter((id) => !shown.includes(id));
}

/** Move the item at `index` one place up (-1) or down (+1); returns the
 * new list and the item's new index. */
export function moveItem<T>(list: T[], index: number, delta: -1 | 1): { list: T[]; index: number } {
  const to = index + delta;
  if (index < 0 || index >= list.length || to < 0 || to >= list.length) return { list, index };
  const next = [...list];
  [next[index], next[to]] = [next[to], next[index]];
  return { list: next, index: to };
}

export function toggle(list: number[], id: number, on: boolean): number[] {
  const rest = list.filter((x) => x !== id);
  return on ? rest : [...rest, id];
}
