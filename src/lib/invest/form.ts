// The investment entry form (INV-030): which fields each action shows,
// and typed text → the `InvInput` the engine takes. Parsing only; the
// rules (what each action needs, lot choice, date order) are the
// engine's, and its messages are shown as they come.

import { dateExample, displayDate, parseDate } from "../format/date";
import { formatMoney, parseMoney } from "../format/money";
import { formatPrice, formatQuantity, parsePrice, parseQuantity } from "../format/quantity";
import type {
  AccountId,
  InvAction,
  InvInput,
  LotPick,
  SecurityId,
  Target,
} from "../types/bindings";

type Need = "required" | "optional" | "none";

export interface ActionInfo {
  value: InvAction;
  label: string;
  security: Need;
  shares: boolean;
  price: boolean;
  commission: boolean;
  amount: Need;
  amountLabel: string;
  split: boolean;
  toAccount: boolean;
  /** Takes shares out of lots (FIFO or chosen lots). */
  lots: boolean;
  acquired: boolean;
  /** Cash in/out: an account or category; misc: a category. */
  counterpart: "account_or_category" | "category" | "none";
}

const base: Omit<ActionInfo, "value" | "label"> = {
  security: "required",
  shares: false,
  price: false,
  commission: false,
  amount: "required",
  amountLabel: "Amount",
  split: false,
  toAccount: false,
  lots: false,
  acquired: false,
  counterpart: "none",
};

const a = (value: InvAction, label: string, over: Partial<ActionInfo> = {}): ActionInfo => ({
  ...base,
  value,
  label,
  ...over,
});

const trade = { shares: true, price: true, amount: "optional" as Need };

/** Every action, in the order the entry form lists them (INV-010). */
export const ACTIONS: ActionInfo[] = [
  a("buy", "Buy", { ...trade, commission: true, amountLabel: "Total cost" }),
  a("sell", "Sell", { ...trade, commission: true, lots: true, amountLabel: "Net proceeds" }),
  a("dividend", "Dividend"),
  a("interest", "Interest", { security: "optional" }),
  a("reinvest_dividend", "Reinvest dividend", { ...trade, amountLabel: "Amount reinvested" }),
  a("reinvest_cg_short", "Reinvest ST capital gain", { ...trade, amountLabel: "Amount reinvested" }),
  a("reinvest_cg_long", "Reinvest LT capital gain", { ...trade, amountLabel: "Amount reinvested" }),
  a("cg_dist_short", "ST capital gain distribution"),
  a("cg_dist_long", "LT capital gain distribution"),
  a("return_of_capital", "Return of capital"),
  a("split", "Stock split", { amount: "none", split: true }),
  a("transfer_shares", "Transfer shares", { shares: true, amount: "none", toAccount: true, lots: true }),
  a("shares_added", "Shares added", { shares: true, acquired: true, amountLabel: "Cost basis" }),
  a("shares_removed", "Shares removed", { shares: true, amount: "none", lots: true }),
  a("cash_in", "Cash in", { security: "none", counterpart: "account_or_category" }),
  a("cash_out", "Cash out", { security: "none", counterpart: "account_or_category" }),
  a("fee", "Fee", { security: "optional" }),
  a("tax_withholding", "Tax withheld", { security: "optional" }),
  a("misc_income", "Misc income", { security: "optional", counterpart: "category" }),
  a("misc_expense", "Misc expense", { security: "optional", counterpart: "category" }),
];

export function actionInfo(action: InvAction): ActionInfo {
  return ACTIONS.find((x) => x.value === action) ?? ACTIONS[0];
}

export interface InvForm {
  action: InvAction;
  date: string;
  settleDate: string;
  security: SecurityId | null;
  quantity: string;
  price: string;
  commission: string;
  amount: string;
  splitNew: string;
  splitOld: string;
  toAccount: AccountId | null;
  /** "" = the security's or account's default. */
  lotMethod: "" | "fifo" | "specific";
  /** Specific lots: shares typed per lot ID. */
  picks: Record<number, string>;
  acquired: string;
  /** "a:<id>" an account, "c:<id>" a category, "" none. */
  counterpart: string;
  memo: string;
}

export function emptyForm(action: InvAction = "buy", date = ""): InvForm {
  return {
    action,
    date,
    settleDate: "",
    security: null,
    quantity: "",
    price: "",
    commission: "",
    amount: "",
    splitNew: "",
    splitOld: "",
    toAccount: null,
    lotMethod: "",
    picks: {},
    acquired: "",
    counterpart: "",
    memo: "",
  };
}

const targetKey = (t: Target | null): string =>
  t === null ? "" : `${t.kind === "account" ? "a" : "c"}:${t.id}`;

/** A stored transaction's input (from `inv_input`), as the form shows it. */
export function formFromInput(i: InvInput): InvForm {
  const picks: Record<number, string> = {};
  for (const p of i.lots) picks[p.lot] = formatQuantity(p.quantity);
  return {
    action: i.action,
    date: displayDate(i.date),
    settleDate: i.settle_date ? displayDate(i.settle_date) : "",
    security: i.security,
    quantity: i.quantity ? formatQuantity(i.quantity) : "",
    price: i.price ? formatPrice(i.price) : "",
    commission: i.commission === "0.00" ? "" : formatMoney(i.commission),
    amount: i.amount ? formatMoney(i.amount) : "",
    splitNew: i.split ? String(i.split.new) : "",
    splitOld: i.split ? String(i.split.old) : "",
    toAccount: i.to_account,
    lotMethod: i.lot_method === "fifo" || i.lot_method === "specific" ? i.lot_method : "",
    picks,
    acquired: i.acquired ? displayDate(i.acquired) : "",
    counterpart: targetKey(i.counterpart),
    memo: i.memo,
  };
}

export type BuildResult = { ok: true; input: InvInput } | { ok: false; error: string };

function whole(text: string): number | null {
  const t = text.trim();
  return /^\d{1,9}$/.test(t) && Number(t) > 0 ? Number(t) : null;
}

/** Typed form → `InvInput`, or the first thing that does not parse. */
export function buildInput(account: AccountId, f: InvForm, today: string): BuildResult {
  const info = actionInfo(f.action);
  const fail = (error: string): BuildResult => ({ ok: false, error });
  const date = parseDate(f.date, today);
  if (date === null) return fail(`Enter the date, like ${dateExample()}.`);
  let settle: string | null = null;
  if (f.settleDate.trim()) {
    settle = parseDate(f.settleDate, today);
    if (settle === null) return fail(`Enter the settlement date like ${dateExample()}, or leave it empty.`);
  }
  const input: InvInput = {
    account,
    action: f.action,
    date,
    settle_date: settle,
    security: info.security === "none" ? null : f.security,
    quantity: null,
    price: null,
    commission: "0.00",
    amount: null,
    split: null,
    to_account: info.toAccount ? f.toAccount : null,
    lot_method: null,
    lots: [],
    acquired: null,
    counterpart: null,
    memo: f.memo,
  };
  if (info.security === "required" && f.security === null) return fail("Choose a security.");
  if (info.shares) {
    input.quantity = parseQuantity(f.quantity);
    if (input.quantity === null) return fail("Enter the number of shares, like 10 or 12.345.");
  }
  if (info.price && f.price.trim()) {
    input.price = parsePrice(f.price);
    if (input.price === null) return fail("Enter the price per share, like 41.25.");
  }
  if (info.commission && f.commission.trim()) {
    const c = parseMoney(f.commission);
    if (c === null) return fail("Enter the commission as an amount, like 4.95.");
    input.commission = c;
  }
  if (info.amount !== "none" && f.amount.trim()) {
    input.amount = parseMoney(f.amount);
    if (input.amount === null) return fail(`Enter the ${info.amountLabel.toLowerCase()} as an amount, like 1,234.56.`);
  }
  if (info.split) {
    const n = whole(f.splitNew);
    const o = whole(f.splitOld);
    if (n === null || o === null) return fail("Enter the split as whole numbers of shares: new for old, like 2 for 1.");
    input.split = { new: n, old: o };
  }
  if (info.lots) {
    if (f.lotMethod) input.lot_method = f.lotMethod;
    if (f.lotMethod === "specific") {
      const lots: LotPick[] = [];
      for (const [lot, text] of Object.entries(f.picks)) {
        if (!text.trim()) continue;
        const q = parseQuantity(text);
        if (q === null) return fail("Enter the shares from each chosen lot, like 5 or 2.5.");
        lots.push({ lot: Number(lot), quantity: q });
      }
      input.lots = lots;
    }
  }
  if (info.acquired && f.acquired.trim()) {
    input.acquired = parseDate(f.acquired, today);
    if (input.acquired === null) return fail(`Enter the acquisition date like ${dateExample()}, or leave it empty.`);
  }
  if (info.counterpart !== "none" && f.counterpart) {
    const [kind, id] = f.counterpart.split(":");
    input.counterpart = { kind: kind === "a" ? "account" : "category", id: Number(id) };
  }
  return { ok: true, input };
}
