// Register grid keyboard behavior (REG-030, UI-050) as pure functions, so
// it is testable without a DOM (TEST-120).

export type RowAction =
  | "up"
  | "down"
  | "first"
  | "last"
  | "page-up"
  | "page-down"
  | "edit"
  | "delete"
  | "toggle-clear"
  | "new"
  | "menu";

export function rowKeyAction(
  e: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey">,
): RowAction | null {
  if (e.altKey || e.metaKey) return null;
  if (e.ctrlKey) return e.key === "n" || e.key === "N" ? "new" : null;
  if (e.shiftKey && e.key === "F10") return "menu";
  switch (e.key) {
    case "ArrowUp":
      return "up";
    case "ArrowDown":
      return "down";
    case "Home":
      return "first";
    case "End":
      return "last";
    case "PageUp":
      return "page-up";
    case "PageDown":
      return "page-down";
    case "Enter":
      return "edit";
    case "Delete":
      return "delete";
    case " ":
      return "toggle-clear";
    case "Insert":
      return "new";
    case "ContextMenu":
      return "menu";
    default:
      return null;
  }
}

/** The row to select after a movement action; stays put at the ends. */
export function moveSelection<T>(
  ids: T[],
  current: T | null,
  action: RowAction,
  pageRows = 10,
): T | null {
  if (ids.length === 0) return null;
  const i = current === null ? -1 : ids.indexOf(current);
  const last = ids.length - 1;
  switch (action) {
    case "up":
      return ids[i <= 0 ? 0 : i - 1];
    case "down":
      return ids[Math.min(last, i + 1)];
    case "first":
      return ids[0];
    case "last":
      return ids[last];
    case "page-up":
      return ids[Math.max(0, (i < 0 ? 0 : i) - pageRows)];
    case "page-down":
      return ids[Math.min(last, Math.max(0, i) + pageRows)];
    default:
      return current;
  }
}
