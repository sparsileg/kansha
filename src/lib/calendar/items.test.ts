import { describe, expect, it } from "vitest";
import { fromTxn, mergeItems } from "./items";
import type { CalendarTxn, OccurrenceView } from "../types/bindings";

const occ = (schedule: number, date: string, over: Partial<OccurrenceView> = {}): OccurrenceView => ({
  schedule,
  nominal: date,
  date,
  amount: "-10.00",
  direction: "payment",
  status: "pending",
  account: 2,
  payee: null,
  estimated: false,
  mode: "remind",
  overridden: false,
  txn: null,
  needs_review: false,
  overdue: false,
  actionable: true,
  ...over,
});
const txn = (id: number, date: string, account = 2): CalendarTxn => ({ txn: id, date, account, payee: null, amount: "5.00" });

describe("calendar items (CAL-020)", () => {
  it("a register transaction is posted, never actionable, keyed by transaction and account", () => {
    const t = fromTxn(txn(7, "2026-09-02", 3));
    expect(t).toMatchObject({ key: "t7-3", status: "posted", actionable: false, overdue: false, occ: null, txn: 7 });
  });

  it("merges by date, scheduled items first on a day, each list in its own order", () => {
    const items = mergeItems(
      [occ(1, "2026-09-02"), occ(2, "2026-09-05", { status: "entered", txn: 9 })],
      [txn(8, "2026-09-01"), txn(10, "2026-09-02", 2), txn(10, "2026-09-02", 3)],
    );
    expect(items.map((i) => i.key)).toEqual(["t8-2", "1-2026-09-02", "t10-2", "t10-3", "2-2026-09-05"]);
    expect(items[4].occ?.schedule).toBe(2);
  });
});
