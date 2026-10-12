// Edit > Find/Replace (REG-170): the window's choices, its Find rows as a
// query, row keys for the checkboxes, and what Replace All says. Matching
// and replacing happen in Rust.

import { datePattern, parseDate } from "../format/date";
import type {
  FindComparison,
  FindCondition,
  FindField,
  FindJoin,
  FindQuery,
  FindSort,
  FoundKey,
  ReplaceResult,
} from "../types/bindings";

/** The Find and Replace field choices, in menu order. */
export const FIELDS: { value: FindField; label: string }[] = [
  { value: "amount", label: "Amount" },
  { value: "category", label: "Category" },
  { value: "check_num", label: "Check #" },
  { value: "cleared", label: "Cleared status" },
  { value: "date", label: "Date" },
  { value: "memo", label: "Memo" },
  { value: "payee", label: "Payee" },
  { value: "tag", label: "Tag" },
];

export const COMPARISONS: { value: FindComparison; label: string }[] = [
  { value: "contains", label: "Contains" },
  { value: "exactly", label: "Exactly" },
  { value: "starts_with", label: "Starts with" },
  { value: "ends_with", label: "Ends with" },
  { value: "greater", label: "Greater" },
  { value: "greater_or_equal", label: "Greater or equal" },
  { value: "less", label: "Less" },
  { value: "less_or_equal", label: "Less or equal" },
];

/** Cleared status names, as Find compares them. Replace offers the
 * first two: only reconciling marks a transaction Reconciled. */
export const CLEARED = ["Unmarked", "Cleared", "Reconciled"] as const;

/** One Find row as the window holds it. */
export type FindRow = { field: FindField; comparison: FindComparison; value: string };

export const newRow = (): FindRow => ({ field: "payee", comparison: "contains", value: "" });

const TEXT_COMPARISONS: FindComparison[] = ["contains", "starts_with", "ends_with"];

/**
 * The query for the window's rows. A Date row compared as a date
 * (Exactly, Greater, Less) has its typed date read in the user's format
 * and sent as ISO; one that is not a date throws, naming the format.
 */
export function buildQuery(
  rows: FindRow[],
  join: FindJoin,
  withinSplits: boolean,
  sort: FindSort,
  descending: boolean,
  today: string,
): FindQuery {
  const conditions: FindCondition[] = rows.map((r) => {
    if (r.field !== "date" || TEXT_COMPARISONS.includes(r.comparison) || r.value.trim() === "") {
      return { ...r };
    }
    const iso = parseDate(r.value, today);
    if (iso === null) throw new Error(`Date: “${r.value.trim()}” is not a date (${datePattern()}).`);
    return { ...r, value: iso };
  });
  return { conditions, join, within_splits: withinSplits, sort, descending };
}

/** What Replace All sends for a typed value: a date as ISO. */
export function replaceValue(field: FindField, typed: string, today: string): string {
  if (field !== "date") return typed;
  const iso = parseDate(typed, today);
  if (iso === null) throw new Error(`“${typed.trim()}” is not a date (${datePattern()}).`);
  return iso;
}

/** A row's checkbox key. */
export function keyId(k: FoundKey): string {
  return `${k.txn}:${k.account}:${k.line ?? ""}`;
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** What Replace All did, for the window. */
export function replaceMessage(r: ReplaceResult): string {
  const parts = [`Changed ${plural(r.changed, "transaction", "transactions")}.`];
  if (r.unchanged > 0) parts.push(`${r.unchanged} already had that value.`);
  const skipped: string[] = [];
  if (r.skipped_split > 0) skipped.push(plural(r.skipped_split, "split", "splits"));
  if (r.skipped_transfer > 0) skipped.push(plural(r.skipped_transfer, "transfer", "transfers"));
  if (r.skipped_closed > 0) skipped.push(`${r.skipped_closed} in a closed account`);
  if (r.skipped_gone > 0) skipped.push(`${r.skipped_gone} voided or deleted`);
  if (skipped.length > 0) parts.push(`Skipped ${skipped.join(", ")}.`);
  return parts.join(" ");
}
