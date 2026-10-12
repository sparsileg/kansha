// The Start Reconciliation form: typed text → the `StartInput` the engine
// takes. Parsing only; the rules (a positive amount, a date not after the
// statement) are the engine's, and its messages are shown as they come.

import { dateExample, parseDate } from "../format/date";
import { parseMoney } from "../format/money";
import type { Account, AccountId, CategoryId, StartInput, StatementItem } from "../types/bindings";

/** Interest earned or a service charge, as typed. Blank amount = none. */
export interface ItemForm {
  date: string;
  amount: string;
  category: CategoryId | null;
}

export interface StartForm {
  statementDate: string;
  /** Ending balance as the statement shows it: money owed on a credit
   * card is positive. The engine converts to ledger sign. */
  balance: string;
  interest: ItemForm;
  service: ItemForm;
}

export const emptyItem = (): ItemForm => ({ date: "", amount: "", category: null });

export const emptyStartForm = (): StartForm => ({
  statementDate: "",
  balance: "",
  interest: emptyItem(),
  service: emptyItem(),
});

export type StartResult = { ok: true; input: StartInput } | { ok: false; error: string };

function item(
  label: string,
  f: ItemForm,
  statementDate: string,
  today: string,
): StatementItem | null | string {
  if (f.amount.trim() === "") return null;
  const amount = parseMoney(f.amount);
  if (amount === null) return `${label}: enter an amount, like 1,234.56.`;
  if (f.category === null) return `${label}: choose a category.`;
  const date = f.date.trim() === "" ? statementDate : parseDate(f.date, today);
  if (date === null) return `${label}: enter a date, like ${dateExample()}.`;
  return { date, amount, category: f.category };
}

export function buildStart(
  account: AccountId,
  form: StartForm,
  today: string,
  interestLabel = "Interest",
): StartResult {
  const statementDate = parseDate(form.statementDate, today);
  if (statementDate === null) {
    return { ok: false, error: `Enter the statement date, like ${dateExample()}.` };
  }
  const balance = parseMoney(form.balance);
  if (balance === null) {
    return { ok: false, error: "Enter the statement ending balance, like 1,234.56." };
  }
  const interest = item(interestLabel, form.interest, statementDate, today);
  if (typeof interest === "string") return { ok: false, error: interest };
  const service = item("Service charge", form.service, statementDate, today);
  if (typeof service === "string") return { ok: false, error: service };
  return {
    ok: true,
    input: {
      account,
      statement_date: statementDate,
      statement_balance: balance,
      interest,
      service_charge: service,
    },
  };
}

/** Accounts a statement can be reconciled against: banking and credit
 * card accounts, and investment accounts that keep their own cash. The
 * engine has the same rule (`reconcile::is_reconcilable`) and refuses any
 * other. */
const RECONCILABLE = new Set(["checking", "savings", "cash", "money_market", "credit_card"]);

export const isReconcilable = (a: Pick<Account, "account_type" | "investment">): boolean =>
  RECONCILABLE.has(a.account_type) || a.investment?.cash_mode === "internal";

/** The reconcile lists' and interest box's names for an account, as in
 * Quicken (RCN-020): a bank's Payments and checks and Deposits; a
 * credit card's Charges and cash advances, Payments and credits, and
 * Finance charges; a cash account's Amounts spent and received; an
 * investment account's Cash out and Cash in. `checkedOut` and
 * `checkedIn` follow "Checked" in the summary. */
export type ReconcileLabels = {
  outs: string;
  ins: string;
  checkedOut: string;
  checkedIn: string;
  interest: string;
  /** Names the interest item in messages ("Finance charge: …"). */
  interestItem: string;
};

export function reconcileLabels(a: Pick<Account, "account_type" | "investment"> | undefined): ReconcileLabels {
  if (a?.account_type === "credit_card")
    return {
      outs: "Charges and cash advances",
      ins: "Payments and credits",
      checkedOut: "charges",
      checkedIn: "payments and credits",
      interest: "Finance charges",
      interestItem: "Finance charge",
    };
  const bank = { interest: "Interest earned", interestItem: "Interest" };
  if (a?.account_type === "cash")
    return { ...bank, outs: "Amounts spent", ins: "Amounts received", checkedOut: "amounts spent", checkedIn: "amounts received" };
  if (a?.investment != null)
    return { ...bank, outs: "Cash out", ins: "Cash in", checkedOut: "cash out", checkedIn: "cash in" };
  return { ...bank, outs: "Payments and checks", ins: "Deposits", checkedOut: "payments and checks", checkedIn: "deposits" };
}
