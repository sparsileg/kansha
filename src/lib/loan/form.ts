// The loan screen's draft (LOAN-010): strings as typed, converted to
// and from `LoanSave`. Presentation only: payments, splits, and the
// amortization table are worked out in Rust (`loan_preview`).

import { displayDate, parseDate } from "../format/date";
import { parseMoney } from "../format/money";
import type {
  AccountId,
  Compounding,
  LoanDetails,
  LoanRate,
  LoanSave,
  LoanType,
  PaymentFrequency,
} from "../types/bindings";

export const LOAN_TYPES: [LoanType, string][] = [
  ["mortgage", "Mortgage"],
  ["auto", "Auto"],
  ["consumer", "Consumer"],
  ["student", "Student"],
  ["commercial", "Commercial"],
  ["business", "Business"],
  ["construction", "Construction"],
  ["home_equity", "Home equity"],
];

export const COMPOUNDING: [Compounding, string][] = [
  ["daily", "Daily"],
  ["monthly", "Monthly"],
  ["semi_annually", "Semi-annually"],
];

export const FREQUENCIES: [PaymentFrequency, string][] = [
  ["twice_per_year", "Twice per year"],
  ["quarterly", "Quarterly"],
  ["every_other_month", "Every other month"],
  ["monthly", "Monthly"],
  ["twice_per_month", "Twice per month"],
  ["every_other_week", "Every other week"],
  ["weekly", "Weekly"],
];

export interface OtherDraft {
  /** A category ID, as text; empty when unset. */
  category: string;
  amount: string;
}

export interface LoanDraft {
  account: AccountId;
  loanType: LoanType;
  opening: string;
  original: string;
  /** The rate in effect, as typed (percent). */
  rate: string;
  /** When a changed rate takes effect (LOAN-050). */
  rateFrom: string;
  /** The saved history; empty for a new loan. */
  rates: LoanRate[];
  length: string;
  lengthUnit: "years" | "months";
  compounding: Compounding;
  frequency: PaymentFrequency;
  day1: string;
  day2: string;
  current: string;
  /** The lender's payment is typed instead of worked out. */
  typed: boolean;
  payment: string;
  nextDue: string;
  lender: string;
  payingAccount: string;
  interestCategory: string;
  others: OtherDraft[];
  extra: string;
}

export function newLoanDraft(account: AccountId): LoanDraft {
  return {
    account,
    loanType: "mortgage",
    opening: "",
    original: "",
    rate: "",
    rateFrom: "",
    rates: [],
    length: "30",
    lengthUnit: "years",
    compounding: "monthly",
    frequency: "monthly",
    day1: "1",
    day2: "15",
    current: "",
    typed: false,
    payment: "",
    nextDue: "",
    lender: "",
    payingAccount: "",
    interestCategory: "",
    others: [],
    extra: "",
  };
}

/** "6.250" and "6.25" are the same rate. */
function sameRate(a: string, b: string): boolean {
  const norm = (s: string) => {
    const t = s.trim();
    return t.includes(".") ? t.replace(/0+$/, "").replace(/\.$/, "") : t;
  };
  return norm(a) === norm(b);
}

/** A saved loan as a draft; `lender` is the payee's name. */
export function draftFromDetails(d: LoanDetails, lender: string): LoanDraft {
  const f = d.loan.fields;
  const current = f.rates[f.rates.length - 1];
  const years = f.length_months % 12 === 0;
  return {
    account: f.account,
    loanType: f.loan_type,
    opening: displayDate(f.opening_date),
    original: f.original_balance,
    rate: current?.rate ?? "",
    rateFrom: d.next_due ? displayDate(d.next_due) : "",
    rates: f.rates,
    length: String(years ? f.length_months / 12 : f.length_months),
    lengthUnit: years ? "years" : "months",
    compounding: f.compounding,
    frequency: f.frequency,
    day1: String(f.day1 ?? 1),
    day2: String(f.day2 ?? 15),
    current: d.owed,
    typed: f.payment !== null,
    payment: f.payment ?? "",
    nextDue: d.next_due ? displayDate(d.next_due) : "",
    lender,
    payingAccount: String(f.paying_account),
    interestCategory: String(f.interest_category),
    others: f.others.map((o) => ({ category: String(o.category), amount: o.amount })),
    extra: f.extra_principal === "0.00" ? "" : f.extra_principal,
  };
}

/** The rate was changed from the one in effect: it needs a date. */
export function rateChanged(d: LoanDraft): boolean {
  const last = d.rates[d.rates.length - 1];
  return last !== undefined && !sameRate(last.rate, d.rate);
}

export type Built =
  | { ok: true; save: LoanSave; lenderName: string }
  | { ok: false; error: string };

const fail = (error: string): Built => ({ ok: false, error });

function whole(s: string): number | null {
  const t = s.trim();
  return /^\d+$/.test(t) ? Number(t) : null;
}

/** The draft as `loan_save` takes it, or what is missing. */
export function buildSave(d: LoanDraft, today: string): Built {
  const opening = parseDate(d.opening, today);
  if (opening === null) return fail("Enter the opening date.");
  const original = parseMoney(d.original);
  if (original === null || original.startsWith("-")) return fail("Enter the original balance.");
  const rate = d.rate.trim();
  if (!/^\d+(\.\d{1,6})?$/.test(rate) && !/^\.\d{1,6}$/.test(rate)) {
    return fail("Enter the interest rate in percent, such as 6.25.");
  }
  const n = whole(d.length);
  if (n === null || n < 1) return fail("Enter the loan's length.");
  const months = d.lengthUnit === "years" ? n * 12 : n;
  const nextDue = parseDate(d.nextDue, today);
  if (nextDue === null) return fail("Enter the next payment's due date.");
  const current = d.current.trim() === "" ? original : parseMoney(d.current);
  if (current === null || current.startsWith("-")) return fail("Enter the current balance.");
  let payment: string | null = null;
  if (d.typed) {
    payment = parseMoney(d.payment);
    if (payment === null || payment.startsWith("-")) return fail("Enter the lender's payment.");
  }
  const extra = d.extra.trim() === "" ? "0.00" : parseMoney(d.extra);
  if (extra === null || extra.startsWith("-")) return fail("Enter the extra principal, or leave it empty.");
  if (d.payingAccount === "") return fail("Choose the account payments come from.");
  if (d.interestCategory === "") return fail("Choose the interest category.");
  const others = [];
  for (const o of d.others) {
    if (o.category === "" && o.amount.trim() === "") continue;
    const amount = parseMoney(o.amount);
    if (o.category === "" || amount === null || amount.startsWith("-")) {
      return fail("Each Other line needs a category and an amount.");
    }
    others.push({ category: Number(o.category), amount });
  }
  let day1: number | null = null;
  let day2: number | null = null;
  if (d.frequency === "twice_per_month") {
    day1 = whole(d.day1);
    day2 = whole(d.day2);
    if (day1 === null || day2 === null) return fail("Enter the two days of the month.");
  }
  let rates: LoanRate[];
  if (d.rates.length === 0) {
    rates = [{ effective: opening, rate }];
  } else if (rateChanged(d)) {
    const from = parseDate(d.rateFrom, today);
    if (from === null) return fail("Enter the date the new rate takes effect.");
    rates = [...d.rates.filter((r) => r.effective < from), { effective: from, rate }];
  } else {
    rates = d.rates;
  }
  return {
    ok: true,
    lenderName: d.lender.trim(),
    save: {
      fields: {
        account: d.account,
        loan_type: d.loanType,
        opening_date: opening,
        original_balance: original,
        length_months: months,
        compounding: d.compounding,
        frequency: d.frequency,
        day1,
        day2,
        rates,
        payment,
        extra_principal: extra,
        others,
        lender: null,
        paying_account: Number(d.payingAccount),
        interest_category: Number(d.interestCategory),
      },
      next_due: nextDue,
      current_balance: current,
    },
  };
}
