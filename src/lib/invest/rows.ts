// The Investments screen's rows, flattened from Rust's overview: account
// rows, then positions and (when expanded) their open lots and, when
// asked for, their sales; then cash, then Totals. Only display
// formatting happens here; every figure comes from Rust.

import { displayDate } from "../format/date";
import { formatMoney } from "../format/money";
import { formatPrice, formatQuantity } from "../format/quantity";
import type { Portfolio, PortfolioPosition, PortfolioTotals } from "../types/bindings";
import type { ColumnId } from "./views";

export type RowKind = "account" | "cash" | "position" | "lot" | "sale" | "total";

export interface Row {
  key: string;
  kind: RowKind;
  name: string;
  /** Text per column; a missing column is blank. */
  cells: Partial<Record<ColumnId, string>>;
  /** Expandable rows: the key to toggle, and the state. */
  toggleKey?: string;
  expanded?: boolean;
  /** Account rows: the account to open. */
  account?: number;
  /** Position rows: the security, for its details. */
  security?: number;
  /** A tooltip for the market value: prices missing or old. */
  warn?: string;
  /** Text to flag the price cell: an old price. */
  priceWarn?: string;
}

const money = (m: string | null | undefined) => (m == null ? "" : formatMoney(m));
const percent = (p: string | null | undefined) => (p == null ? "" : `${p}%`);

export const accountKey = (account: number) => `a${account}`;
export const positionKey = (account: number, security: number) => `p${account}:${security}`;

function totalCells(t: PortfolioTotals): Row["cells"] {
  return {
    cost_basis: money(t.basis),
    market_value: money(t.market_value),
    gain: money(t.gain),
    day_gain: money(t.day_gain),
    day_percent: percent(t.day_percent),
  };
}

function warning(t: PortfolioTotals): string | undefined {
  if (t.missing_prices) return "Some holdings have no price and are left out.";
  if (t.stale_prices) return "Some prices are out of date (older than the stale-price limit).";
  return undefined;
}

function positionCells(p: PortfolioPosition): Row["cells"] {
  return {
    ticker: p.ticker ?? "",
    price: p.price ? formatPrice(p.price) : "no price",
    shares: formatQuantity(p.shares),
    cost_basis: money(p.basis),
    market_value: money(p.market_value),
    gain: money(p.gain),
    day_gain: money(p.day_gain),
    day_percent: percent(p.day_percent),
  };
}

export function buildRows(
  pf: Portfolio,
  expanded: ReadonlySet<string>,
  accountName: (id: number) => string,
): Row[] {
  const rows: Row[] = [];
  for (const a of pf.accounts) {
    const ak = accountKey(a.account);
    const open = expanded.has(ak);
    rows.push({
      key: ak,
      kind: "account",
      name: accountName(a.account),
      account: a.account,
      toggleKey: ak,
      expanded: open,
      // Collapsed: the account's rolled-up figures.
      cells: open ? {} : totalCells(a.totals),
      warn: open ? undefined : warning(a.totals),
    });
    if (!open) continue;
    for (const p of a.positions) {
      const pk = positionKey(a.account, p.security);
      const popen = expanded.has(pk);
      rows.push({
        key: pk,
        kind: "position",
        name: p.name,
        security: p.security,
        toggleKey: pk,
        expanded: popen,
        cells: popen ? { ticker: p.ticker ?? "" } : positionCells(p),
        priceWarn: !popen && p.stale ? `Price of ${p.price_date ? displayDate(p.price_date) : "?"} is out of date (older than the stale-price limit)` : undefined,
      });
      if (!popen) continue;
      for (const l of p.lots) {
        rows.push({
          key: `l${l.lot}`,
          kind: "lot",
          name: `Lot ${displayDate(l.acquired)}`,
          cells: {
            price: p.price ? formatPrice(p.price) : "",
            shares: formatQuantity(l.shares),
            cost_basis: money(l.basis),
            market_value: money(l.market_value),
            gain: money(l.gain),
            day_gain: money(l.day_gain),
            day_percent: percent(p.day_percent),
          },
        });
      }
      // Sales (Show closed lots): proceeds under Market Value, the
      // realized gain under Gain/Loss.
      for (const x of p.sales) {
        rows.push({
          key: `s${x.lot}:${x.sold}:${x.shares}`,
          kind: "sale",
          name: `Sold ${displayDate(x.sold)} (lot ${displayDate(x.acquired)})`,
          cells: {
            shares: formatQuantity(x.shares),
            cost_basis: money(x.basis),
            market_value: money(x.proceeds),
            gain: money(x.gain),
          },
        });
      }
    }
    // Cash closes the account, below its equities.
    if (a.cash !== null) {
      rows.push({ key: `c${a.account}`, kind: "cash", name: "Cash", cells: { market_value: money(a.cash) } });
    }
  }
  rows.push({ key: "total", kind: "total", name: "Totals:", cells: totalCells(pf.total), warn: warning(pf.total) });
  return rows;
}
