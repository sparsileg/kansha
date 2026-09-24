// Date-only handling for financial dates. Integer calendar arithmetic on
// ISO "YYYY-MM-DD" strings; never a JS `Date` (spec §17.3). "Today" is
// always passed in from the Rust clock (`commands.today`).

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

/** ISO "2026-03-05" → "03/05/2026". Invalid input is returned as is. */
export function displayDate(iso: string): string {
  const m = ISO.exec(iso);
  return m ? `${m[2]}/${m[3]}/${m[1]}` : iso;
}

/**
 * Parse a typed date into ISO, or `null`. Accepts "3/5/2026", "3/5/26",
 * "3/5" (year from `today`), "2026-03-05". Two-digit years are 20yy.
 */
export function parseDate(input: string, today: string): string | null {
  const s = input.trim();
  if (isValidIso(s)) return s;
  const m = /^(\d{1,2})[/.-](\d{1,2})(?:[/.-](\d{2}|\d{4}))?$/.exec(s);
  if (!m) return null;
  let y = +today.slice(0, 4);
  if (m[3]) y = m[3].length === 2 ? 2000 + +m[3] : +m[3];
  return make(y, +m[1], +m[2]);
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
