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
