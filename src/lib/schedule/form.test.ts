import { describe, expect, it } from "vitest";
import { buildFields, draftFromFields, newScheduleDraft } from "./form";

const TODAY = "2026-09-24";

function filled() {
  const d = newScheduleDraft(1, "2026-10-01");
  d.lines = [{ target: "c:5", amount: "1,000.00", memo: "", tag: "" }];
  return d;
}

describe("buildFields", () => {
  it("builds a monthly payment with a negative amount", () => {
    const r = buildFields(filled(), TODAY);
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.fields.lines[0].amount).toBe("-1000.00");
    expect(r.fields.lines[0].target).toEqual({ kind: "category", id: 5 });
    expect(r.fields.recurrence).toMatchObject({
      frequency: "monthly",
      interval: 1,
      day1: 1,
      start_date: "2026-10-01",
    });
  });

  it("passes the payee as a name, existing or new", () => {
    const d = filled();
    d.payee = "  Brand New Landlord ";
    const r = buildFields(d, TODAY);
    expect(r.ok && r.payeeName).toBe("Brand New Landlord");
    expect(r.ok && r.fields.payee).toBeNull();
  });

  it("uses a positive amount for a deposit", () => {
    const d = filled();
    d.direction = "deposit";
    const r = buildFields(d, TODAY);
    expect(r.ok && r.fields.lines[0].amount).toBe("1000.00");
  });

  it("maps quarterly and twice a year onto monthly intervals", () => {
    const d = filled();
    d.preset = "quarterly";
    let r = buildFields(d, TODAY);
    expect(r.ok && r.fields.recurrence.interval).toBe(3);
    d.preset = "twice_yearly";
    r = buildFields(d, TODAY);
    expect(r.ok && r.fields.recurrence.interval).toBe(6);
  });

  it("maps last day, nth weekday, twice monthly", () => {
    const d = filled();
    d.preset = "last_day";
    let r = buildFields(d, TODAY);
    expect(r.ok && r.fields.recurrence.frequency).toBe("monthly_last_day");
    d.preset = "nth_weekday";
    d.weekday = "2";
    d.weekOfMonth = "-1";
    r = buildFields(d, TODAY);
    expect(r.ok && r.fields.recurrence).toMatchObject({
      frequency: "monthly_nth_weekday",
      weekday: 2,
      week_of_month: -1,
    });
    d.preset = "twice_monthly";
    d.day1 = "15";
    d.day2 = "15";
    expect(buildFields(d, TODAY).ok).toBe(false);
    d.day2 = "31";
    r = buildFields(d, TODAY);
    expect(r.ok && r.fields.recurrence).toMatchObject({ day1: 15, day2: 31 });
  });

  it("parses end conditions", () => {
    const d = filled();
    d.endKind = "after_count";
    d.count = "6";
    let r = buildFields(d, TODAY);
    expect(r.ok && r.fields.end).toEqual({ kind: "after_count", count: 6 });
    d.endKind = "on_date";
    d.endDate = "12/31/2027";
    r = buildFields(d, TODAY);
    expect(r.ok && r.fields.end).toEqual({ kind: "on_date", date: "2027-12-31" });
    d.endDate = "nope";
    expect(buildFields(d, TODAY).ok).toBe(false);
  });

  it("reports what is missing", () => {
    const d = filled();
    d.account = "";
    expect(buildFields(d, TODAY)).toEqual({ ok: false, error: "Choose an account." });
    const e = filled();
    e.lines[0].amount = "";
    expect(buildFields(e, TODAY).ok).toBe(false);
    const f = filled();
    f.lines[0].target = "";
    expect(buildFields(f, TODAY).ok).toBe(false);
  });
});

describe("memo with one line or several", () => {
  it("one line: the transaction memo is the only memo", () => {
    const d = filled();
    d.memo = "rent";
    d.lines[0].memo = "stale";
    const r = buildFields(d, TODAY);
    expect(r.ok && r.fields.memo).toBe("rent");
    expect(r.ok && r.fields.lines[0].memo).toBe("");
  });

  it("one line with only a line memo: it becomes the transaction memo", () => {
    const d = filled();
    d.lines[0].memo = "kept";
    const r = buildFields(d, TODAY);
    expect(r.ok && r.fields.memo).toBe("kept");
    expect(r.ok && r.fields.lines[0].memo).toBe("");
  });

  it("several lines keep both kinds of memo", () => {
    const d = filled();
    d.memo = "whole";
    d.lines[0].memo = "first";
    d.lines.push({ target: "c:6", amount: "5.00", memo: "second", tag: "" });
    const r = buildFields(d, TODAY);
    expect(r.ok && r.fields.memo).toBe("whole");
    expect(r.ok && r.fields.lines.map((l) => l.memo)).toEqual(["first", "second"]);
  });

  it("loading a saved single line with only a line memo shows it in the top memo", () => {
    const d = filled();
    const r = buildFields(d, TODAY);
    if (!r.ok) throw new Error(r.error);
    r.fields.memo = "";
    r.fields.lines[0].memo = "old line memo";
    const back = draftFromFields(r.fields, "");
    expect(back.memo).toBe("old line memo");
  });
});

describe("draftFromFields", () => {
  it("round-trips a schedule", () => {
    const d = filled();
    d.payee = "Landlord";
    d.preset = "quarterly";
    d.mode = "auto";
    d.estimated = true;
    const r = buildFields(d, TODAY);
    if (!r.ok) throw new Error(r.error);
    const back = draftFromFields(r.fields, "Landlord");
    expect(back.payee).toBe("Landlord");
    expect(back.preset).toBe("quarterly");
    expect(back.direction).toBe("payment");
    expect(back.lines[0].amount).toBe("1000.00");
    const again = buildFields(back, TODAY);
    expect(again).toEqual(r);
  });
});
