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
