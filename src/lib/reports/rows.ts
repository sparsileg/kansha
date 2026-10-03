// A report's row tree as the lines the table shows. An expanded group
// shows a heading line, its rows, and a closing "Total" line carrying its
// figures, or, with totals on the heading (RPT-020), a heading line
// carrying them and no closing line; a collapsed group shows one line
// with its figures.

import type { Drill, Row, RowKind } from "../types/bindings";

export interface Line {
  /** Unique per line (the group's path, plus ":end" for its total). */
  key: string;
  /** The group's path for toggling; null for other lines. */
  path: string | null;
  depth: number;
  kind: RowKind;
  label: string;
  /** Empty for an expanded group's heading, unless totals go there. */
  cells: string[];
  drill: Drill | null;
  collapsed: boolean;
  /** An expanded group's closing total line. */
  closing: boolean;
}

/** A group's closing label; a label that already says "Total" gets no
 * second one. */
export const closingLabel = (label: string): string => (label.startsWith("Total ") ? label : `Total ${label}`);

export function flatten(rows: Row[], isCollapsed: (path: string) => boolean, onHeading = false): Line[] {
  const out: Line[] = [];
  const walk = (list: Row[], prefix: string, depth: number) => {
    list.forEach((r, i) => {
      const path = prefix ? `${prefix}/${i}` : String(i);
      const group = r.children.length > 0;
      if (!group) {
        out.push({ key: path, path: null, depth, kind: r.kind, label: r.label, cells: r.cells, drill: r.drill, collapsed: false, closing: false });
        return;
      }
      const collapsed = isCollapsed(path);
      out.push({
        key: path,
        path,
        depth,
        kind: r.kind,
        label: r.label,
        cells: collapsed || onHeading ? r.cells : r.cells.map(() => ""),
        drill: r.drill,
        collapsed,
        closing: false,
      });
      if (collapsed) return;
      walk(r.children, path, depth + 1);
      if (onHeading) return;
      out.push({
        key: `${path}:end`,
        path: null,
        depth,
        kind: r.kind,
        label: closingLabel(r.label),
        cells: r.cells,
        drill: r.drill,
        collapsed: false,
        closing: true,
      });
    });
  };
  walk(rows, "", 0);
  return out;
}
