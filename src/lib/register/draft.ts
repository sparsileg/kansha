// The register's entry row as plain data, and its conversion to and from
// the IPC `Entry`. Pure: no IPC, no Svelte, no money arithmetic (amounts
// are parsed and signed by string operations in `format/money`). Covered
// by Vitest (TEST-120).

import { applyDateKey, displayDate, parseDate } from "../format/date";
import {
  combinePaymentDeposit,
  negateMoney,
  parseMoney,
  splitPaymentDeposit,
} from "../format/money";
import type {
  AccountId,
  Cleared,
  Entry,
  EntryLine,
  Payee,
  TagId,
  Target,
} from "../types/bindings";

/** Picker value: "" none, "c:<id>" category, "a:<id>" transfer, "split". */
export type TargetValue = string;

export const SPLIT = "split";

export function targetValue(t: Target): TargetValue {
  return t.kind === "category" ? `c:${t.id}` : `a:${t.id}`;
}

export function parseTargetValue(v: TargetValue): Target | null {
  const m = /^([ca]):(\d+)$/.exec(v);
  if (!m) return null;
  return m[1] === "c"
    ? { kind: "category", id: Number(m[2]) }
    : { kind: "account", id: Number(m[2]) };
}

export interface SplitDraft {
  target: TargetValue;
  /** Typed magnitude; the direction comes from Payment vs Deposit. */
  amount: string;
  memo: string;
  cleared: Cleared;
  tags: TagId[];
}

export interface Draft {
  date: string;
  check_num: string;
  payee: string;
  payment: string;
  deposit: string;
  category: TargetValue;
  tag: string;
  memo: string;
  notes: string;
  cleared: Cleared;
  /** Carried through a simple (one-line) entry unchanged. */
  lineCleared: Cleared;
  lineTags: TagId[];
  splits: SplitDraft[];
}

export function newDraft(today: string, date?: string): Draft {
  return {
    date: displayDate(date ?? today),
    check_num: "",
    payee: "",
    payment: "",
    deposit: "",
    category: "",
    tag: "",
    memo: "",
    notes: "",
    cleared: "unmarked",
    lineCleared: "unmarked",
    lineTags: [],
    splits: [],
  };
}

function unsigned(canonical: string): string {
  return canonical.startsWith("-") ? canonical.slice(1) : canonical;
}

export function draftFromEntry(entry: Entry, payeeName: string): Draft {
  const { payment, deposit } = splitPaymentDeposit(entry.amount);
  const d = newDraft("", entry.date);
  d.date = displayDate(entry.date);
  d.check_num = entry.check_num;
  d.payee = payeeName;
  d.payment = payment;
  d.deposit = deposit;
  d.memo = entry.memo;
  d.notes = entry.notes;
  d.cleared = entry.cleared;
  d.tag = entry.tags.length ? String(entry.tags[0]) : "";
  if (entry.lines.length === 1) {
    const l = entry.lines[0];
    d.category = targetValue(l.target);
    d.lineCleared = l.cleared;
    d.lineTags = l.tags;
  } else if (entry.lines.length > 1) {
    d.category = SPLIT;
    d.splits = entry.lines.map((l) => ({
      target: targetValue(l.target),
      amount: unsigned(l.amount),
      memo: l.memo,
      cleared: l.cleared,
      tags: l.tags,
    }));
  }
  return d;
}

/** Nothing typed except (possibly) the date: Enter on it is a no-op. */
export function isBlank(d: Draft): boolean {
  return [d.check_num, d.payee, d.payment, d.deposit, d.memo, d.tag].every(
    (v) => v.trim() === "",
  ) && d.category === "";
}

export function emptySplit(): SplitDraft {
  return { target: "", amount: "", memo: "", cleared: "unmarked", tags: [] };
}

/** Whether the split direction is a payment (negative) for this draft. */
function isPayment(d: Draft): boolean {
  return d.payment.trim() !== "";
}

/** Signed canonical amount of one split line, or null if not a number. */
export function splitAmount(d: Draft, s: SplitDraft): string | null {
  const m = parseMoney(s.amount);
  if (m === null) return null;
  return isPayment(d) ? negateMoney(unsigned(m)) : m;
}

/**
 * The arguments for the `split_remainder` command, or null while the
 * total or any part is empty or invalid.
 */
export function splitParts(
  d: Draft,
): { total: string; parts: string[] } | null {
  const total = combinePaymentDeposit(d.payment, d.deposit);
  if (total === null) return null;
  const parts: string[] = [];
  for (const s of d.splits) {
    if (s.amount.trim() === "") continue;
    const a = splitAmount(d, s);
    if (a === null) return null;
    parts.push(a);
  }
  return { total, parts };
}

/**
 * Fill empty fields from a payee's memorized defaults (PAY-020). Never
 * overwrites what the user has typed.
 */
export function applyQuickFill(d: Draft, p: Payee): Draft {
  const out = { ...d };
  if (out.category === "" && p.default_category !== null) {
    out.category = `c:${p.default_category}`;
  }
  if (out.tag === "" && p.default_tag !== null) out.tag = String(p.default_tag);
  if (out.memo === "" && p.default_memo) out.memo = p.default_memo;
  if (out.payment.trim() === "" && out.deposit.trim() === "") {
    if (p.default_amount !== null) {
      const { payment, deposit } = splitPaymentDeposit(p.default_amount);
      out.payment = payment;
      out.deposit = deposit;
    }
  }
  return out;
}

/** Typing in Payment clears Deposit and vice versa (D-10). */
export function setAmountField(
  d: Draft,
  field: "payment" | "deposit",
  value: string,
): Draft {
  return field === "payment"
    ? { ...d, payment: value, deposit: value.trim() ? "" : d.deposit }
    : { ...d, deposit: value, payment: value.trim() ? "" : d.payment };
}

/** Date-field keys (REG-030); returns the new field text or null. */
export function dateFieldKey(
  key: string,
  text: string,
  today: string,
): string | null {
  const cur = parseDate(text, today) ?? "";
  const next = applyDateKey(key, cur, today);
  return next === null ? null : displayDate(next);
}

export type Built =
  | { ok: true; entry: Entry; payeeName: string }
  | { ok: false; error: string };

/**
 * Turn the draft into an `Entry`. Split remainder is checked by the
 * caller through `split_remainder`; everything else is checked here.
 */
export function buildEntry(
  d: Draft,
  account: AccountId,
  today: string,
): Built {
  const date = parseDate(d.date, today);
  if (date === null) return { ok: false, error: "Date is not valid." };
  const amount = combinePaymentDeposit(d.payment, d.deposit);
  if (amount === null) {
    return {
      ok: false,
      error:
        d.payment.trim() && d.deposit.trim()
          ? "Enter a payment or a deposit, not both."
          : "Enter a valid payment or deposit amount.",
    };
  }
  if (d.category === "") {
    return {
      ok: false,
      error: "Choose a category, a transfer account, or Split.",
    };
  }
  const tags: TagId[] = d.tag ? [Number(d.tag)] : [];
  let lines: EntryLine[];
  if (d.category === SPLIT) {
    lines = [];
    for (const s of d.splits) {
      if (s.amount.trim() === "" && s.target === "") continue;
      const target = parseTargetValue(s.target);
      const a = splitAmount(d, s);
      if (target === null || a === null) {
        return {
          ok: false,
          error: "Every split line needs a category or account and an amount.",
        };
      }
      lines.push({
        target,
        amount: a,
        memo: s.memo,
        cleared: target.kind === "account" ? s.cleared : "unmarked",
        tags: s.tags,
      });
    }
    if (lines.length < 2) {
      return { ok: false, error: "A split needs at least two lines." };
    }
  } else {
    const target = parseTargetValue(d.category);
    if (target === null) return { ok: false, error: "Category is not valid." };
    lines = [
      {
        target,
        amount,
        memo: "",
        cleared: target.kind === "account" ? d.lineCleared : "unmarked",
        tags: d.lineTags,
      },
    ];
  }
  return {
    ok: true,
    payeeName: d.payee.trim(),
    entry: {
      account,
      date,
      payee: null,
      check_num: d.check_num.trim(),
      memo: d.memo,
      notes: d.notes,
      amount,
      cleared: d.cleared,
      tags,
      lines,
    },
  };
}
