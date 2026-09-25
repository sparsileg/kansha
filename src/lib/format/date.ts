// Date-only handling for financial dates. Integer calendar arithmetic on
// ISO "YYYY-MM-DD" strings; never a JS `Date` (spec §17.3). "Today" is
// always passed in from the Rust clock (`commands.today`). Shown and typed
// dates follow the user's date format (SET-030).

import { dateFormatState } from "../state/dateformat.svelte";

const ISO = /^(\d{4})-(\d{2})-(\d{2})$/;

export function isLeap(y: number): boolean {
  return (y % 4 === 0 && y % 100 !== 0) || y % 400 === 0;
}

export function daysInMonth(y: number, m: number): number {
  if (m === 2) return isLeap(y) ? 29 : 28;
  return [4, 6, 9, 11].includes(m) ? 30 : 31;
}

const pad = (n: number, w = 2) => String(n).padStart(w, "0");

function make(y: number, m: number, d: number): string | null {
  if (y < 1 || y > 9999 || m < 1 || m > 12) return null;
  if (d < 1 || d > daysInMonth(y, m)) return null;
  return `${pad(y, 4)}-${pad(m)}-${pad(d)}`;
}

export function isValidIso(s: string): boolean {
  const m = ISO.exec(s);
  return !!m && make(+m[1], +m[2], +m[3]) === s;
}

// Days from civil (Howard Hinnant's algorithm), integer only.
function toDays(y: number, m: number, d: number): number {
  y -= m <= 2 ? 1 : 0;
  const era = Math.floor(y / 400);
  const yoe = y - era * 400;
  const doy = Math.floor((153 * (m + (m > 2 ? -3 : 9)) + 2) / 5) + d - 1;
  const doe = yoe * 365 + Math.floor(yoe / 4) - Math.floor(yoe / 100) + doy;
  return era * 146097 + doe - 719468;
}

function fromDays(z: number): [number, number, number] {
  z += 719468;
  const era = Math.floor(z / 146097);
  const doe = z - era * 146097;
  const yoe = Math.floor(
    (doe - Math.floor(doe / 1460) + Math.floor(doe / 36524) - Math.floor(doe / 146096)) / 365,
  );
  const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100));
  const mp = Math.floor((5 * doy + 2) / 153);
  const d = doy - Math.floor((153 * mp + 2) / 5) + 1;
  const m = mp + (mp < 10 ? 3 : -9);
  return [yoe + era * 400 + (m <= 2 ? 1 : 0), m, d];
}

/** Add (or subtract) whole days. Returns `iso` unchanged if it is invalid. */
export function addDays(iso: string, n: number): string {
  const m = ISO.exec(iso);
  if (!m) return iso;
  const [y, mo, d] = fromDays(toDays(+m[1], +m[2], +m[3]) + n);
  return make(y, mo, d) ?? iso;
}

/** ISO "2026-03-05" in the user's format: "03/05/2026", "05/03/2026",
 * or "2026-03-05". Invalid input is returned as is. */
export function displayDate(iso: string): string {
  const m = ISO.exec(iso);
  if (!m) return iso;
  switch (dateFormatState.value) {
    case "dmy":
      return `${m[3]}/${m[2]}/${m[1]}`;
    case "ymd":
      return iso;
    default:
      return `${m[2]}/${m[3]}/${m[1]}`;
  }
}

/** The format's pattern, for placeholders: "MM/DD/YYYY" and so on. */
export function datePattern(): string {
  switch (dateFormatState.value) {
    case "dmy":
      return "DD/MM/YYYY";
    case "ymd":
      return "YYYY-MM-DD";
    default:
      return "MM/DD/YYYY";
  }
}

/** An example date in the user's format, for messages. */
export const dateExample = (): string => displayDate("2026-03-31");

/**
 * Parse a typed date into ISO, or `null`, in the user's format. A
 * four-digit year first ("2026-3-5", "2026/03/05") is always accepted.
 * Otherwise:
 * - MM/DD/YYYY: "3/5/2026", "3/5/26", "3/5" (year from `today`).
 * - DD/MM/YYYY: "5/3/2026", "5/3/26", "5/3".
 * - YYYY-MM-DD: "3-5" or "3/5" (month-day, year from `today`).
 * Two-digit years are 20yy. `/`, `.`, and `-` all separate.
 */
export function parseDate(input: string, today: string): string | null {
  const s = input.trim();
  const ymd = /^(\d{4})[/.-](\d{1,2})[/.-](\d{1,2})$/.exec(s);
  if (ymd) return make(+ymd[1], +ymd[2], +ymd[3]);
  const format = dateFormatState.value;
  const m =
    format === "ymd"
      ? /^(\d{1,2})[/.-](\d{1,2})()$/.exec(s)
      : /^(\d{1,2})[/.-](\d{1,2})(?:[/.-](\d{2}|\d{4}))?$/.exec(s);
  if (!m) return null;
  let y = +today.slice(0, 4);
  if (m[3]) y = m[3].length === 2 ? 2000 + +m[3] : +m[3];
  return format === "dmy" ? make(y, +m[2], +m[1]) : make(y, +m[1], +m[2]);
}

/**
 * Register date-field keys (REG-030): `+`/`=` next day, `-` previous day,
 * `t` today. Returns the new ISO date, or `null` if `key` is not a date
 * key. `current` falls back to `today` when empty or invalid.
 */
export function applyDateKey(
  key: string,
  current: string,
  today: string,
): string | null {
  const base = isValidIso(current) ? current : today;
  switch (key) {
    case "+":
    case "=":
      return addDays(base, 1);
    case "-":
      return addDays(base, -1);
    case "t":
    case "T":
      return today;
    default:
      return null;
  }
}

/** Day of week, 0 = Sunday … 6 = Saturday. Returns -1 for invalid input. */
export function weekdayOf(iso: string): number {
  const m = ISO.exec(iso);
  if (!m || make(+m[1], +m[2], +m[3]) !== iso) return -1;
  return (((toDays(+m[1], +m[2], +m[3]) + 4) % 7) + 7) % 7;
}

/** The first day of the month containing `iso`. */
export function monthStart(iso: string): string {
  return `${iso.slice(0, 7)}-01`;
}

/** The first day of the month `n` months after (or before) `iso`'s month. */
export function addMonths(iso: string, n: number): string {
  const index = Number(iso.slice(0, 4)) * 12 + (Number(iso.slice(5, 7)) - 1) + n;
  return `${pad(Math.floor(index / 12), 4)}-${pad((index % 12) + 1)}-01`;
}

/** "September 2026" for the month containing `iso`. */
export function monthLabel(iso: string): string {
  const names = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
  ];
  return `${names[Number(iso.slice(5, 7)) - 1]} ${iso.slice(0, 4)}`;
}

/** Six Sunday-first weeks (42 ISO dates) covering the month of `iso`. */
export function monthGrid(iso: string): string[] {
  const first = monthStart(iso);
  const start = addDays(first, -weekdayOf(first));
  return Array.from({ length: 42 }, (_, i) => addDays(start, i));
}
