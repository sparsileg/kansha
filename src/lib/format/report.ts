// Report cells and headings as text. Cells arrive from Rust as canonical
// strings (money "-1234.50", dates ISO, quantities "12.5"); only display
// formatting happens here.

import type { Column, ColumnKind } from "../types/bindings";
import { displayDate } from "./date";
import { formatMoney, formatMoneyWhole } from "./money";
import { formatQuantity } from "./quantity";

export function formatCell(kind: ColumnKind, text: string, cents: boolean): string {
  if (text === "") return "";
  switch (kind) {
    case "money":
      return cents ? formatMoney(text) : formatMoneyWhole(text);
    case "date":
      return displayDate(text);
    case "quantity":
      return formatQuantity(text);
    default:
      return text;
  }
}

/** A column heading's lines: a balance column shows its date over
 * "Balance"; a period column without a name shows its dates. */
export function columnHeading(c: Column): string[] {
  if (c.to !== null && c.label === "Balance") return [displayDate(c.to), c.label];
  if (c.label === "" && c.to !== null) {
    return c.from !== null && c.from !== c.to
      ? [displayDate(c.from), `– ${displayDate(c.to)}`]
      : [displayDate(c.to)];
  }
  return [c.label];
}
