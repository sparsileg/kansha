// The schedule form's draft: strings as typed, converted to and from
// `ScheduleFields`. Presentation only. Recurrence dates and amounts are
// computed in Rust; this maps the friendly frequency names ("Quarterly")
// onto the engine's (frequency, interval) pairs and parses typed values.

import { parseDate } from "../format/date";
import { negateMoney, parseMoney, splitPaymentDeposit } from "../format/money";
import { parseTargetValue, targetValue } from "../register/draft";
import type {
  AccountId,
  End,
  EntryMode,
  ScheduleFields,
  WeekendRule,
} from "../types/bindings";

export type Preset =
  | "once"
  | "daily"
  | "weekly"
  | "twice_monthly"
  | "monthly"
  | "quarterly"
  | "twice_yearly"
  | "last_day"
  | "nth_weekday"
  | "yearly";

export const PRESETS: [Preset, string][] = [
  ["once", "Only once"],
  ["daily", "Daily"],
  ["weekly", "Weekly (every N weeks)"],
  ["twice_monthly", "Twice a month"],
  ["monthly", "Monthly on a day"],
  ["last_day", "Monthly on the last day"],
  ["nth_weekday", "Monthly on the Nth weekday"],
  ["quarterly", "Quarterly"],
  ["twice_yearly", "Twice a year"],
  ["yearly", "Yearly"],
];

export const WEEKDAYS = [
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
  "Sunday",
];

export interface LineDraft {
  /** `c:ID` or `a:ID`, as `TargetCombo` uses. Empty when unset. */
  target: string;
  /** Magnitude as typed; the direction is the form's. */
  amount: string;
  memo: string;
  tag: string;
}

export interface ScheduleDraft {
  account: string;
  payee: string;
  memo: string;
  estimated: boolean;
  direction: "payment" | "deposit";
  lines: LineDraft[];
  preset: Preset;
  every: string;
  day1: string;
  day2: string;
  weekday: string;
  weekOfMonth: string;
  start: string;
  weekendRule: WeekendRule;
  endKind: "never" | "on_date" | "after_count";
  endDate: string;
  count: string;
  remindDays: string;
  mode: EntryMode;
}

export const emptyLine = (): LineDraft => ({
  target: "",
  amount: "",
  memo: "",
  tag: "",
});

/** A blank monthly schedule starting on `start` in `account`. */
export function newScheduleDraft(
  account: AccountId | null,
  start: string,
): ScheduleDraft {
  return {
    account: account === null ? "" : String(account),
    payee: "",
    memo: "",
    estimated: false,
    direction: "payment",
    lines: [emptyLine()],
    preset: "monthly",
    every: "1",
    day1: start ? String(Number(start.slice(8, 10))) : "1",
    day2: "15",
    weekday: "1",
    weekOfMonth: "1",
    start,
    weekendRule: "none",
    endKind: "never",
    endDate: "",
    count: "12",
    remindDays: "3",
    mode: "remind",
  };
}

function presetOf(f: ScheduleFields): Preset {
  const r = f.recurrence;
  switch (r.frequency) {
    case "monthly":
      return r.interval === 3
        ? "quarterly"
        : r.interval === 6
          ? "twice_yearly"
          : "monthly";
    case "monthly_last_day":
      return "last_day";
    case "monthly_nth_weekday":
      return "nth_weekday";
    default:
      return r.frequency;
  }
}

/** The form's draft for an existing (or prefilled) schedule. `payeeName`
 * is the payee's name, since the form types a name (new ones are created
 * on save). */
export function draftFromFields(f: ScheduleFields, payeeName = ""): ScheduleDraft {
  const r = f.recurrence;
  const first = f.lines[0];
  const sign = first ? splitPaymentDeposit(first.amount) : null;
  const direction = sign && sign.deposit !== "" ? "deposit" : "payment";
  const preset = presetOf(f);
  const fixedInterval = preset === "quarterly" || preset === "twice_yearly";
  const mag = (a: string) => a.replace(/^-/, "");
  return {
    account: String(f.account),
    payee: payeeName,
    // One line has no memo of its own; the form shows the transaction memo.
    memo: f.memo || (f.lines.length === 1 ? f.lines[0].memo : ""),
    estimated: f.amount_type === "estimated",
    direction,
    lines: f.lines.map((l) => ({
      target: targetValue(l.target),
      amount: mag(l.amount),
      memo: l.memo,
      tag: l.tag === null ? "" : String(l.tag),
    })),
    preset,
    every: fixedInterval ? "1" : String(r.interval),
    day1: r.day1 === null ? String(Number(r.start_date.slice(8, 10))) : String(r.day1),
    day2: r.day2 === null ? "15" : String(r.day2),
    weekday: String(r.weekday ?? 1),
    weekOfMonth: String(r.week_of_month ?? 1),
    start: r.start_date,
    weekendRule: r.weekend_rule,
    endKind: f.end.kind,
    endDate: f.end.kind === "on_date" ? f.end.date : "",
    count: f.end.kind === "after_count" ? String(f.end.count) : "12",
    remindDays: String(f.remind_days),
    mode: f.mode,
  };
}

export type BuiltSchedule =
  | { ok: true; fields: ScheduleFields; payeeName: string }
  | { ok: false; error: string };

const fail = (error: string): BuiltSchedule => ({ ok: false, error });

function whole(s: string, min: number, max: number): number | null {
  if (!/^\d+$/.test(s.trim())) return null;
  const n = Number(s);
  return n >= min && n <= max ? n : null;
}

/** Parse the draft into `ScheduleFields`, or say what is wrong. */
export function buildFields(d: ScheduleDraft, today: string): BuiltSchedule {
  const account = whole(d.account, 1, Number.MAX_SAFE_INTEGER);
  if (account === null) return fail("Choose an account.");
  const start = parseDate(d.start, today);
  if (start === null) return fail("Enter a valid start date.");

  if (d.lines.length === 0) return fail("Add a category or transfer account.");
  const lines = [];
  for (const [i, l] of d.lines.entries()) {
    const target = parseTargetValue(l.target);
    if (target === null) return fail(`Line ${i + 1}: choose a category or account.`);
    const parsed = parseMoney(l.amount);
    if (parsed === null || l.amount.trim() === "")
      return fail(`Line ${i + 1}: enter an amount.`);
    const mag = parsed.replace(/^-/, "");
    lines.push({
      target,
      amount: d.direction === "payment" ? negateMoney(mag) : mag,
      // With one line the form shows only the transaction memo.
      memo: d.lines.length === 1 ? "" : l.memo,
      tag: l.tag === "" ? null : Number(l.tag),
    });
  }

  const every = whole(d.every, 1, 1200);
  let frequency: ScheduleFields["recurrence"]["frequency"];
  let interval = 1;
  let day1: number | null = null;
  let day2: number | null = null;
  let weekday: number | null = null;
  let weekOfMonth: number | null = null;
  const needEvery = () => every === null;
  switch (d.preset) {
    case "once":
      frequency = "once";
      break;
    case "twice_monthly":
      frequency = "twice_monthly";
      day1 = whole(d.day1, 1, 31);
      day2 = whole(d.day2, 1, 31);
      if (day1 === null || day2 === null) return fail("Enter two days of the month (1-31).");
      if (day1 >= day2) return fail("The first day must come before the second.");
      break;
    case "monthly":
    case "quarterly":
    case "twice_yearly":
      frequency = "monthly";
      day1 = whole(d.day1, 1, 31);
      if (day1 === null) return fail("Enter a day of the month (1-31).");
      if (d.preset === "quarterly") interval = 3;
      else if (d.preset === "twice_yearly") interval = 6;
      else if (needEvery()) return fail("Enter how many months apart (1 or more).");
      else interval = every ?? 1;
      break;
    case "nth_weekday":
      frequency = "monthly_nth_weekday";
      weekday = whole(d.weekday, 1, 7);
      weekOfMonth = d.weekOfMonth === "-1" ? -1 : whole(d.weekOfMonth, 1, 4);
      if (weekday === null || weekOfMonth === null) return fail("Choose the week and weekday.");
      if (needEvery()) return fail("Enter how many months apart (1 or more).");
      interval = every ?? 1;
      break;
    default:
      frequency = d.preset === "last_day" ? "monthly_last_day" : d.preset;
      if (needEvery()) return fail("Enter how often it repeats (1 or more).");
      interval = every ?? 1;
  }

  let end: End = { kind: "never" };
  if (d.endKind === "on_date") {
    const date = parseDate(d.endDate, today);
    if (date === null) return fail("Enter a valid end date.");
    end = { kind: "on_date", date };
  } else if (d.endKind === "after_count") {
    const count = whole(d.count, 0, 100000);
    if (count === null) return fail("Enter how many occurrences remain.");
    end = { kind: "after_count", count };
  }

  const remind = whole(d.remindDays, 0, 365);
  if (remind === null) return fail("Remind days must be 0 to 365.");

  return {
    ok: true,
    payeeName: d.payee.trim(),
    fields: {
      account,
      // The payee is set by name on save (`payeeName`); Rust finds or
      // creates it.
      payee: null,
      memo: d.memo !== "" || d.lines.length !== 1 ? d.memo : d.lines[0].memo,
      amount_type: d.estimated ? "estimated" : "fixed",
      lines,
      recurrence: {
        frequency,
        interval,
        day1,
        day2,
        weekday,
        week_of_month: weekOfMonth,
        start_date: start,
        weekend_rule: d.weekendRule,
      },
      end,
      remind_days: remind,
      mode: d.mode,
    },
  };
}
