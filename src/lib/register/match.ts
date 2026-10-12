// Type-ahead matching for the Category field (REG-030). The user types
// "checking", not "[Checking]": brackets are display only, and a query
// matches categories and transfer accounts alike.

export interface TargetOption {
  /** Picker value: "c:<id>", "a:<id>", or "split". */
  value: string;
  /** What the register shows: "Food:Groceries", "[Checking]", "--Split--". */
  label: string;
}

const plain = (s: string) => s.toLowerCase().replace(/[[\]]/g, "");

/**
 * Options that contain every word of `query` (case-insensitive, brackets
 * ignored), best first: label starts with the query, then a path segment
 * or word starts with it, then anywhere. Ties keep the input order. An
 * empty query returns everything in order.
 */
export function matchTargets(
  options: TargetOption[],
  query: string,
  limit = 50,
): TargetOption[] {
  const words = plain(query).split(/\s+/).filter(Boolean);
  if (words.length === 0) return options.slice(0, limit);
  const ranked: { o: TargetOption; score: number; i: number }[] = [];
  options.forEach((o, i) => {
    const hay = plain(o.label);
    if (!words.every((w) => hay.includes(w))) return;
    const first = words[0];
    const score = hay.startsWith(first)
      ? 0
      : hay.split(/[:\s-]+/).some((seg) => seg.startsWith(first))
        ? 1
        : 2;
    ranked.push({ o, score, i });
  });
  ranked.sort((a, b) => a.score - b.score || a.i - b.i);
  return ranked.slice(0, limit).map((r) => r.o);
}

/**
 * The `Parent:Child` path to create when the typed text names no existing
 * category or account, or null when there is nothing to create: empty
 * text, a `[Account]` name, an empty path level ("Charity:"), or a path
 * that already exists (ignoring case and spaces around colons).
 */
export function newCategoryPath(
  options: TargetOption[],
  text: string,
): string | null {
  const t = text.trim();
  if (t === "" || t.startsWith("[") || t.startsWith("--")) return null;
  const parts = t.split(":").map((x) => x.trim());
  if (parts.some((x) => x === "")) return null;
  const path = parts.join(":");
  const key = path.toLowerCase();
  if (options.some((o) => plainLabel(o.label) === key)) return null;
  return path;
}

const plainLabel = (label: string) => label.toLowerCase().replace(/[[\]]/g, "");

/**
 * Quicken-style completion of the path level being typed: when the text
 * before the last ":" names an existing category path and exactly one
 * name below it starts with the rest, the text with that name filled in
 * ("Car:B" → "Car:BlueForester"), plus the option it now names (null if
 * that path is not listed). The top level works the same way ("Ca" →
 * "Car"). Accounts and split take no part. Null when no name or more
 * than one fits, or the name is already whole.
 */
export function completeSegment(
  options: TargetOption[],
  text: string,
): { text: string; value: string | null } | null {
  const t = text.trimStart();
  if (t.startsWith("[") || t.startsWith("--")) return null;
  const cut = text.lastIndexOf(":");
  const parent = cut < 0 ? [] : text.slice(0, cut).split(":").map((x) => x.trim().toLowerCase());
  if (parent.some((x) => x === "")) return null;
  const rest = text.slice(cut + 1).trimStart().toLowerCase();
  if (rest === "") return null;
  const names = new Map<string, string>();
  for (const o of options) {
    if (!o.value.startsWith("c:")) continue;
    const segs = o.label.split(":").map((x) => x.trim());
    if (segs.length <= parent.length) continue;
    if (!parent.every((p, i) => segs[i].toLowerCase() === p)) continue;
    const name = segs[parent.length];
    if (name.toLowerCase().startsWith(rest)) names.set(name.toLowerCase(), name);
  }
  if (names.size !== 1) return null;
  const [name] = names.values();
  if (name.length === rest.length) return null;
  const key = [...parent, name.toLowerCase()].join(":");
  const hit = options.find((o) => o.value.startsWith("c:") && plainLabel(o.label) === key);
  return { text: text + name.slice(rest.length), value: hit?.value ?? null };
}
