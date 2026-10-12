import { describe, expect, it } from "vitest";
import { buildQuery, COMPARISONS, FIELDS, keyId, newRow, replaceMessage, replaceValue } from "./form";

const TODAY = "2026-10-11";

describe("Find/Replace choices (REG-170)", () => {
  it("lists the eight fields and eight comparisons in order", () => {
    expect(FIELDS.map((f) => f.label)).toEqual([
      "Amount",
      "Category",
      "Check #",
      "Cleared status",
      "Date",
      "Memo",
      "Payee",
      "Tag",
    ]);
    expect(COMPARISONS.map((c) => c.label)).toEqual([
      "Contains",
      "Exactly",
      "Starts with",
      "Ends with",
      "Greater",
      "Greater or equal",
      "Less",
      "Less or equal",
    ]);
  });
});

describe("buildQuery", () => {
  it("passes rows through with the one join and sort", () => {
    const rows = [
      { ...newRow(), value: "Netflix" },
      { field: "amount" as const, comparison: "less" as const, value: "-10" },
    ];
    expect(buildQuery(rows, "or", true, "amount", false, TODAY)).toEqual({
      conditions: rows,
      join: "or",
      within_splits: true,
      sort: "amount",
      descending: false,
    });
  });

  it("sends a date compared as a date as ISO, year from today", () => {
    const q = buildQuery(
      [{ field: "date", comparison: "greater", value: "3/5" }],
      "and",
      false,
      "date",
      true,
      TODAY,
    );
    expect(q.conditions[0].value).toBe("2026-03-05");
  });

  it("leaves a date compared as text as typed", () => {
    const q = buildQuery([{ field: "date", comparison: "contains", value: "/2015" }], "and", false, "date", true, TODAY);
    expect(q.conditions[0].value).toBe("/2015");
  });

  it("refuses a date that is not one, naming the format", () => {
    expect(() =>
      buildQuery([{ field: "date", comparison: "exactly", value: "soon" }], "and", false, "date", true, TODAY),
    ).toThrow("“soon” is not a date (MM/DD/YYYY)");
  });

  it("leaves a blank date row for Rust to drop", () => {
    const q = buildQuery([{ field: "date", comparison: "less", value: " " }], "and", false, "date", true, TODAY);
    expect(q.conditions[0].value).toBe(" ");
  });
});

describe("replaceValue", () => {
  it("reads a date, leaves anything else", () => {
    expect(replaceValue("date", "12/31/2025", TODAY)).toBe("2025-12-31");
    expect(replaceValue("tag", "Subscription", TODAY)).toBe("Subscription");
    expect(() => replaceValue("date", "x", TODAY)).toThrow("not a date");
  });
});

describe("keyId", () => {
  it("tells a split line from its row", () => {
    expect(keyId({ txn: 5, account: 2, line: null })).toBe("5:2:");
    expect(keyId({ txn: 5, account: 2, line: 3 })).toBe("5:2:3");
  });
});

describe("replaceMessage", () => {
  const none = {
    changed: 0,
    unchanged: 0,
    skipped_split: 0,
    skipped_transfer: 0,
    skipped_closed: 0,
    skipped_gone: 0,
  };

  it("counts what changed", () => {
    expect(replaceMessage({ ...none, changed: 1 })).toBe("Changed 1 transaction.");
    expect(replaceMessage({ ...none, changed: 7 })).toBe("Changed 7 transactions.");
  });

  it("says what was left alone and why", () => {
    expect(
      replaceMessage({
        changed: 3,
        unchanged: 1,
        skipped_split: 2,
        skipped_transfer: 1,
        skipped_closed: 1,
        skipped_gone: 1,
      }),
    ).toBe(
      "Changed 3 transactions. 1 already had that value. Skipped 2 splits, 1 transfer, 1 in a closed account, 1 voided or deleted.",
    );
  });
});
