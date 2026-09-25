// Share quantities and prices: parsing and display. String operations
// only: no floats, no arithmetic (CONVENTIONS §4, §6). Canonical form is
// the Rust `Quantity`/`Price` string: "-12.5", "0.333333", "320" (up to
// six decimals, trailing zeros trimmed).

const TYPED = /^(\d[\d,]*)?(?:\.(\d*))?$/;

/**
 * Parse what the user typed ("1,000.5", ".25", "$41.25") into a canonical
 * quantity or price string, or `null`. No sign: shares and prices are
 * never negative. More than six decimals is rejected, never rounded.
 */
export function parseScaled(input: string): string | null {
  const s = input.trim().replace(/^\$/, "");
  const m = TYPED.exec(s);
  if (!m) return null;
  const [, intRaw, frac = ""] = m;
  if (intRaw === undefined && frac === "") return null;
  if (frac.length > 6) return null;
  const intPart = (intRaw ?? "").replace(/,/g, "").replace(/^0+(?=\d)/, "") || "0";
  const trimmed = frac.replace(/0+$/, "");
  return trimmed ? `${intPart}.${trimmed}` : intPart;
}

export const parseQuantity = parseScaled;
export const parsePrice = parseScaled;

function group(digits: string): string {
  return digits.replace(/\B(?=(\d{3})+$)/g, ",");
}

/** "1234.5" → "1,234.5"; decimals as stored. */
export function formatQuantity(canonical: string): string {
  const negative = canonical.startsWith("-");
  const body = negative ? canonical.slice(1) : canonical;
  const [i, f] = body.split(".");
  return `${negative ? "-" : ""}${group(i)}${f ? `.${f}` : ""}`;
}

/** A price with at least two decimals: "320" → "320.00", "41.255" as is. */
export function formatPrice(canonical: string): string {
  const [i, f = ""] = canonical.split(".");
  return `${group(i)}.${f.padEnd(2, "0")}`;
}

/** Keep only what a share or price field may hold: digits, commas, one
 * point, at most six decimals. */
export function sanitizeScaledInput(text: string): string {
  const out = text.replace(/[^0-9.,]/g, "");
  const dot = out.indexOf(".");
  if (dot < 0) return out;
  return out.slice(0, dot + 1) + out.slice(dot + 1).replace(/[.,]/g, "").slice(0, 6);
}
