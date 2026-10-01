/**
 * The order of a manager list: visible rows first, then hidden ones, each
 * group alphabetical by `label` (ignoring case).
 */
export function hiddenLast<T extends { hidden: boolean }>(
  rows: readonly T[],
  label: (row: T) => string,
): T[] {
  return [...rows].sort(
    (a, b) =>
      Number(a.hidden) - Number(b.hidden) ||
      label(a).localeCompare(label(b), undefined, { sensitivity: "base" }),
  );
}
