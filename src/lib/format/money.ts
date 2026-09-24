// Money parsing and display. String operations only: no floats, no
// arithmetic on amounts (CONVENTIONS; spec §17.3). Canonical form is the
// Rust `Money` string: "-1234.56", always two decimals.

const TYPED = /^([+-])?(\d[\d,]*)?(?:\.(\d*))?$/;

/**
 * Parse what the user typed ("1,234.56", "-5", ".5", "+7.1") into a
 * canonical amount string, or `null` when it is not a valid amount.
 * More than two decimals is rejected, never rounded.
 */
export function parseMoney(input: string): string | null {
  const s = input.trim().replace(/^\$/, "").replace(/^([+-])\$/, "$1");
  const m = TYPED.exec(s);
  if (!m) return null;
  const [, sign, intRaw, frac = ""] = m;
  if (intRaw === undefined && frac === "") return null;
  if (frac.length > 2) return null;
  const intPart = (intRaw ?? "").replace(/,/g, "").replace(/^0+(?=\d)/, "") || "0";
  const cents = frac.padEnd(2, "0");
  const negative = sign === "-" && !(intPart === "0" && cents === "00");
  return `${negative ? "-" : ""}${intPart}.${cents}`;
}

function group(digits: string): string {
  return digits.replace(/\B(?=(\d{3})+$)/g, ",");
}

/** "1234.5" style canonical string → "1,234.50" ; negative keeps "-". */
export function formatMoney(canonical: string): string {
  const negative = canonical.startsWith("-");
  const body = negative ? canonical.slice(1) : canonical;
  const [i, f = ""] = body.split(".");
  return `${negative ? "-" : ""}${group(i)}.${f.padEnd(2, "0")}`;
}

/** True for "0", "0.00", "-0.00" and the like. */
export function isZeroMoney(canonical: string): boolean {
  return /^-?0+(\.0+)?$/.test(canonical);
}

/** Debit-normal display for liability accounts: flips the sign text. */
export function negateMoney(canonical: string): string {
  if (canonical.startsWith("-")) return canonical.slice(1);
  return isZeroMoney(canonical) ? canonical : `-${canonical}`;
}

/**
 * Split a signed amount into the Payment and Deposit register columns
 * (D-10). Negative → payment (magnitude); positive → deposit; zero → both
 * empty. Values are display-formatted.
 */
export function splitPaymentDeposit(canonical: string): {
  payment: string;
  deposit: string;
} {
  if (isZeroMoney(canonical)) return { payment: "", deposit: "" };
  if (canonical.startsWith("-"))
    return { payment: formatMoney(canonical.slice(1)), deposit: "" };
  return { payment: "", deposit: formatMoney(canonical) };
}

/**
 * Combine the two entry fields into one signed canonical amount, or `null`
 * when both are empty, both are set, or either is invalid. Typing in one
 * field clears the other, so at most one is normally set.
 */
export function combinePaymentDeposit(
  payment: string,
  deposit: string,
): string | null {
  const p = payment.trim() ? parseMoney(payment) : null;
  const d = deposit.trim() ? parseMoney(deposit) : null;
  if (payment.trim() && p === null) return null;
  if (deposit.trim() && d === null) return null;
  if (p !== null && d !== null) return null;
  if (p !== null) return negateMoney(p.replace(/^-/, ""));
  return d;
}
