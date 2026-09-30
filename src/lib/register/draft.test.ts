import { describe, expect, it } from "vitest";
import {
  SPLIT,
  applyQuickFill,
  buildEntry,
  dateFieldKey,
  draftFromEntry,
  newDraft,
  setAmountField,
  setSplitTag,
  splitCleared,
  splitParts,
  splitTagValue,
  type Draft,
} from "./draft";
import type { Entry, Payee } from "../types/bindings";

const today = "2026-09-24";
const base = () => ({ ...newDraft(today), category: "c:5" });

describe("dateFieldKey (REG-030)", () => {
  it("+/-/t adjust the date text", () => {
    expect(dateFieldKey("+", "09/24/2026", today)).toBe("09/25/2026");
    expect(dateFieldKey("-", "09/24/2026", today)).toBe("09/23/2026");
    expect(dateFieldKey("t", "01/01/2020", today)).toBe("09/24/2026");
    expect(dateFieldKey("x", "09/24/2026", today)).toBeNull();
  });
  it("starts from today when the field is empty or bad", () => {
    expect(dateFieldKey("+", "", today)).toBe("09/25/2026");
  });
});

describe("amount fields (D-10)", () => {
  it("typing in one clears the other", () => {
    let d = setAmountField(base(), "deposit", "5");
    d = setAmountField(d, "payment", "7");
    expect(d.deposit).toBe("");
    expect(d.payment).toBe("7");
  });
});

describe("buildEntry", () => {
  it("builds a simple payment", () => {
    const d = { ...base(), payment: "1,234.50", payee: " Acme ", tag: "3" };
    const r = buildEntry(d, 1, today);
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.payeeName).toBe("Acme");
    expect(r.entry.date).toBe("2026-09-24");
    expect(r.entry.amount).toBe("-1234.50");
    expect(r.entry.tags).toEqual([3]);
    expect(r.entry.lines).toEqual([
      { target: { kind: "category", id: 5 }, amount: "-1234.50", memo: "", cleared: "unmarked", tags: [] },
    ]);
  });

  it("builds a deposit and a transfer", () => {
    const r = buildEntry({ ...base(), deposit: "10", category: "a:2" }, 1, today);
    expect(r.ok && r.entry.amount).toBe("10.00");
    expect(r.ok && r.entry.lines[0].target).toEqual({ kind: "account", id: 2 });
  });

  it.each([
    [{ date: "13/45/2026", payment: "1" }, "Date"],
    [{ payment: "" }, "amount"],
    [{ payment: "1", deposit: "2" }, "not both"],
    [{ payment: "abc" }, "amount"],
    [{ payment: "1", category: "" }, "category"],
  ])("rejects %j", (patch, text) => {
    const r = buildEntry({ ...base(), ...patch }, 1, today);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error).toContain(text);
  });
});

describe("splits", () => {
  const split = (): Draft => ({
    ...base(),
    category: SPLIT,
    payment: "100",
    splits: [
      { target: "c:1", amount: "60", memo: "a", cleared: "unmarked" as const, tags: [] },
      { target: "c:2", amount: "30.50", memo: "", cleared: "unmarked" as const, tags: [] },
    ],
  });

  it("signs the parts like the total, for the remainder command", () => {
    expect(splitParts(split())).toEqual({ total: "-100.00", parts: ["-60.00", "-30.50"] });
    const dep = { ...split(), payment: "", deposit: "100" };
    expect(splitParts(dep)).toEqual({ total: "100.00", parts: ["60.00", "30.50"] });
  });

  it("returns null while a part is invalid", () => {
    const d = split();
    d.splits[1].amount = "x";
    expect(splitParts(d)).toBeNull();
  });

  it("a line typed with '-' goes the other way (paycheck deductions)", () => {
    const d: Draft = {
      ...split(),
      payment: "",
      deposit: "2,000",
      splits: [
        { target: "c:1", amount: "2,500", memo: "", cleared: "unmarked", tags: [] },
        { target: "c:2", amount: "-500", memo: "", cleared: "unmarked", tags: [] },
      ],
    };
    expect(splitParts(d)).toEqual({ total: "2000.00", parts: ["2500.00", "-500.00"] });
    const r = buildEntry(d, 1, today);
    expect(r.ok && r.entry.lines.map((l) => l.amount)).toEqual(["2500.00", "-500.00"]);
    const pay = { ...split() };
    pay.splits[1].amount = "-30.50";
    expect(splitParts(pay)?.parts).toEqual(["-60.00", "30.50"]);
  });

  it("a split may total zero: no amount typed; plain lines are deposits", () => {
    const d: Draft = {
      ...split(),
      payment: "",
      splits: [
        { target: "c:1", amount: "100", memo: "", cleared: "unmarked", tags: [] },
        { target: "a:2", amount: "-100", memo: "", cleared: "unmarked", tags: [] },
      ],
    };
    expect(splitParts(d)).toEqual({ total: "0.00", parts: ["100.00", "-100.00"] });
    const r = buildEntry(d, 1, today);
    expect(r.ok && r.entry.amount).toBe("0.00");
    expect(r.ok && r.entry.lines.map((l) => l.amount)).toEqual(["100.00", "-100.00"]);
  });

  it("a simple entry still needs an amount", () => {
    const r = buildEntry({ ...base(), payment: "", deposit: "" }, 1, today);
    expect(r.ok).toBe(false);
  });

  it("builds lines with signed amounts; needs two lines", () => {
    const r = buildEntry(split(), 1, today);
    expect(r.ok && r.entry.lines.map((l) => l.amount)).toEqual(["-60.00", "-30.50"]);
    const one = split();
    one.splits = one.splits.slice(0, 1);
    expect(buildEntry(one, 1, today).ok).toBe(false);
  });

  it("rejects a line with no target", () => {
    const d = split();
    d.splits[0].target = "";
    expect(buildEntry(d, 1, today).ok).toBe(false);
  });

  it("a line's tag picker sets its first tag and keeps any others", () => {
    const s = split().splits[0];
    expect(splitTagValue(s)).toBe("");
    const tagged = setSplitTag(s, "7");
    expect(tagged.tags).toEqual([7]);
    expect(splitTagValue(tagged)).toBe("7");
    const two = { ...s, tags: [7, 9] };
    expect(setSplitTag(two, "8").tags).toEqual([8, 9]);
    expect(setSplitTag(two, "9").tags).toEqual([9]);
    expect(setSplitTag(two, "").tags).toEqual([9]);
    const d = split();
    d.splits[1] = setSplitTag(d.splits[1], "4");
    const r = buildEntry(d, 1, today);
    expect(r.ok && r.entry.lines.map((l) => l.tags)).toEqual([[], [4]]);
  });

  it("shows the cleared mark of transfer lines only, and saves it unchanged", () => {
    const d = split();
    d.splits[0] = { ...d.splits[0], target: "a:3", cleared: "reconciled" };
    d.splits[1] = { ...d.splits[1], cleared: "cleared" }; // a category line: no mark
    expect(d.splits.map(splitCleared)).toEqual(["R", ""]);
    const r = buildEntry(d, 1, today);
    expect(r.ok && r.entry.lines.map((l) => l.cleared)).toEqual(["reconciled", "unmarked"]);
  });
});

describe("applyQuickFill (PAY-020)", () => {
  const payee = {
    id: 1, name: "Acme", default_category: 9, default_tag: 4,
    default_memo: "usual", default_amount: "-25.00", hidden: false, created_at: "",
  } as Payee;

  it("fills empty fields", () => {
    const d = applyQuickFill(newDraft(today), payee);
    expect(d).toMatchObject({ category: "c:9", tag: "4", memo: "usual", payment: "25.00", deposit: "" });
  });

  it("keeps what the user typed", () => {
    const typed = { ...newDraft(today), category: "c:1", memo: "mine", deposit: "3" };
    const d = applyQuickFill(typed, payee);
    expect(d).toMatchObject({ category: "c:1", memo: "mine", deposit: "3", payment: "" });
  });
});

describe("draftFromEntry round trip", () => {
  const entry: Entry = {
    account: 1, date: "2026-03-05", payee: 2, check_num: "101", memo: "m", notes: "n",
    amount: "-1234.50", cleared: "cleared", tags: [3],
    lines: [
      { target: { kind: "category", id: 5 }, amount: "-1000.00", memo: "x", cleared: "unmarked", tags: [] },
      { target: { kind: "account", id: 7 }, amount: "-234.50", memo: "", cleared: "cleared", tags: [] },
    ],
  };

  it("edits a split entry without changing it", () => {
    const d = draftFromEntry(entry, "Acme");
    expect(d.category).toBe(SPLIT);
    expect(d.payment).toBe("1,234.50");
    const r = buildEntry(d, 1, today);
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.entry).toEqual({ ...entry, payee: null });
  });

  it("edits a mixed-sign split without changing it", () => {
    const paycheck: Entry = {
      ...entry,
      amount: "2000.00",
      lines: [
        { target: { kind: "category", id: 5 }, amount: "2500.00", memo: "", cleared: "unmarked", tags: [] },
        { target: { kind: "category", id: 6 }, amount: "-500.00", memo: "", cleared: "unmarked", tags: [] },
      ],
    };
    const d = draftFromEntry(paycheck, "Acme");
    expect(d.splits.map((s) => s.amount)).toEqual(["2500.00", "-500.00"]);
    const r = buildEntry(d, 1, today);
    if (r.ok) expect(r.entry).toEqual({ ...paycheck, payee: null });
    expect(r.ok).toBe(true);
  });

  it("edits a zero-total split without changing it", () => {
    const zero: Entry = {
      ...entry,
      amount: "0.00",
      lines: [
        { target: { kind: "category", id: 5 }, amount: "100.00", memo: "", cleared: "unmarked", tags: [] },
        { target: { kind: "category", id: 6 }, amount: "-100.00", memo: "", cleared: "unmarked", tags: [] },
      ],
    };
    const d = draftFromEntry(zero, "Acme");
    expect([d.payment, d.deposit]).toEqual(["", ""]);
    const r = buildEntry(d, 1, today);
    if (r.ok) expect(r.entry).toEqual({ ...zero, payee: null });
    expect(r.ok).toBe(true);
  });
});
