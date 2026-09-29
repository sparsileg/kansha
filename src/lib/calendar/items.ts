// What the calendar shows on a day (CAL-010, CAL-020): scheduled
// occurrences (due, entered, skipped) and register transactions that did
// not come from a schedule, as one list. Pure: no IPC, no Svelte.

import type { AccountId, CalendarTxn, OccurrenceView, PayeeId, TxnId } from "../types/bindings";

export interface CalItem {
  /** Unique within a day: `schedule-nominal` for an occurrence,
   * `t<txn>-<account>` for a register transaction. */
  key: string;
  date: string;
  account: AccountId;
  payee: PayeeId | null;
  amount: string;
  /** "posted": a register transaction not entered from a schedule. */
  status: "pending" | "entered" | "skipped" | "posted";
  overdue: boolean;
  /** A pending occurrence that can be entered or skipped now. */
  actionable: boolean;
  /** The scheduled occurrence, or null for a register transaction. */
  occ: OccurrenceView | null;
  /** The transaction, once there is one. */
  txn: TxnId | null;
}

export function fromOccurrence(v: OccurrenceView): CalItem {
  return {
    key: `${v.schedule}-${v.nominal}`,
    date: v.date,
    account: v.account,
    payee: v.payee,
    amount: v.amount,
    status: v.status,
    overdue: v.overdue,
    actionable: v.actionable,
    occ: v,
    txn: v.txn,
  };
}

export function fromTxn(t: CalendarTxn): CalItem {
  return {
    key: `t${t.txn}-${t.account}`,
    date: t.date,
    account: t.account,
    payee: t.payee,
    amount: t.amount,
    status: "posted",
    overdue: false,
    actionable: false,
    occ: null,
    txn: t.txn,
  };
}

/** Both lists by date; on a day, scheduled items first, each list keeping
 * its own order. */
export function mergeItems(occ: OccurrenceView[], txns: CalendarTxn[]): CalItem[] {
  const all = [...occ.map(fromOccurrence), ...txns.map(fromTxn)];
  return all
    .map((item, i) => ({ item, i }))
    .sort((a, b) => (a.item.date < b.item.date ? -1 : a.item.date > b.item.date ? 1 : a.i - b.i))
    .map(({ item }) => item);
}
