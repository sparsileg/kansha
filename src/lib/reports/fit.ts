// Fitting a compact report (RPT-145) to the page. Columns start at their
// natural widths; when they are too wide, Description, Memo, and Tag are
// cut first, together (the widest first), then Account and Tax Item, each
// down to a floor. Category and the other columns are never cut. What still does
// not fit is scaled down on paper; on screen it scrolls. Widths are in
// em, so they hold at any font size.
//
// WebKitGTK ignores page-break rules inside a table and does not repeat
// its heading row on each page, so paper gets one table per page
// (`paginate`), each with the column headings.

/** Columns cut first, together; then the next set. */
export const CUT_ORDER: string[][] = [
  ["description", "memo", "tag"],
  ["account", "tax_item"],
];
/** The narrowest a cut column gets, in em. */
export const MIN_CUT = 5;

export interface Fit {
  /** Column widths, in em. */
  widths: number[];
  /** Font scale (1 or less) so the widths fit. */
  scale: number;
}

const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 0);

/** Narrow the columns at `at` by `excess` in all, the widest first, none
 * below its floor (or its own width, if less). */
function cut(widths: number[], at: number[], excess: number): void {
  const floor = (i: number) => Math.min(widths[i], MIN_CUT);
  const capped = (i: number, level: number) => Math.min(widths[i], Math.max(level, floor(i)));
  const saved = (level: number) => sum(at.map((i) => widths[i] - capped(i, level)));
  let lo = 0;
  let hi = Math.max(0, ...at.map((i) => widths[i]));
  if (saved(lo) <= excess) hi = lo;
  else {
    // The highest level that saves enough.
    for (let n = 0; n < 50; n++) {
      const mid = (lo + hi) / 2;
      if (saved(mid) >= excess) lo = mid;
      else hi = mid;
    }
    hi = lo;
  }
  for (const i of at) widths[i] = capped(i, hi);
}

/** Fit columns (`ids`, natural widths in em) into `avail` em. */
export function fitColumns(ids: string[], natural: number[], avail: number): Fit {
  const widths = [...natural];
  for (const set of CUT_ORDER) {
    const excess = sum(widths) - avail;
    if (excess <= 0) break;
    const at = ids.flatMap((id, i) => (set.includes(id) ? [i] : []));
    cut(widths, at, excess);
  }
  const total = sum(widths);
  return { widths, scale: total > avail && total > 0 ? avail / total : 1 };
}

/** A row of the table, for `paginate`. */
export interface PageRow {
  /** Height, in the unit of the page. */
  h: number;
  /** A group or form heading, which never ends a page. */
  heading: boolean;
}

/** Split rows into pages of `page` height, each starting with the column
 * headings (`head` high); the first page has `first` less room (the
 * title). Headings at the bottom of a page move to the next with the
 * rows under them. Returns each page's first row. */
export function paginate(rows: PageRow[], head: number, page: number, first: number): number[] {
  const starts = [0];
  let start = 0;
  let room = page - first - head;
  rows.forEach((r, i) => {
    if (r.h > room && i > start) {
      let at = i;
      while (at - 1 > start && rows[at - 1].heading) at--;
      starts.push(at);
      start = at;
      room = page - head - sum(rows.slice(at, i).map((x) => x.h));
    }
    room -= r.h;
  });
  return starts;
}

/** The printable size of a Letter page, in pt, as WebKitGTK prints it
 * (measured: about 545 × 685 portrait, 712 × 515 landscape), less a
 * margin for rounding. */
export const PAGE_WIDTH_PT = { portrait: 530, landscape: 700 } as const;
export const PAGE_HEIGHT_PT = { portrait: 665, landscape: 495 } as const;
/** The report's title on the first page, in pt: a 12 pt title and 9 pt
 * dates; a note adds a line. */
export const TITLE_PT = 45;
export const NOTE_PT = 13;
/** Report text size on paper (RPT-050). */
export const PRINT_FONT_PT = 9;
/** Text at 9 pt runs a little wider than on screen; measured widths get
 * this much more. */
export const WIDTH_SLACK = 1.03;
