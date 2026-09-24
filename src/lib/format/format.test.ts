import { describe, expect, it } from "vitest";
import {
  sanitizeAmountInput,
  combinePaymentDeposit,
  formatMoney,
  parseMoney,
  splitPaymentDeposit,
} from "./money";
import { addDays, applyDateKey, displayDate, parseDate } from "./date";

describe("parseMoney", () => {
  it.each([
    ["1,234.56", "1234.56"],
    ["-5", "-5.00"],
    [".5", "0.50"],
    ["+7.1", "7.10"],
    ["$12", "12.00"],
    ["-$3.25", "-3.25"],
    ["007", "7.00"],
    ["-0", "0.00"],
    ["5.", "5.00"],
  ])("%s → %s", (i, o) => expect(parseMoney(i)).toBe(o));

  it.each(["", ".", "abc", "1.234", "1..2", "--5", "1 2"])(
    "rejects %j",
    (i) => expect(parseMoney(i)).toBeNull(),
  );

  it("is exact beyond float range", () => {
    expect(parseMoney("90071992547409.93")).toBe("90071992547409.93");
  });
});

describe("formatMoney", () => {
  it("groups thousands", () => {
    expect(formatMoney("1234567.80")).toBe("1,234,567.80");
    expect(formatMoney("-1234.56")).toBe("-1,234.56");
    expect(formatMoney("0.05")).toBe("0.05");
  });
});

describe("payment/deposit", () => {
  it("splits by sign", () => {
    expect(splitPaymentDeposit("-1234.50")).toEqual({ payment: "1,234.50", deposit: "" });
    expect(splitPaymentDeposit("20.00")).toEqual({ payment: "", deposit: "20.00" });
    expect(splitPaymentDeposit("0.00")).toEqual({ payment: "", deposit: "" });
  });

  it("combines to a signed amount", () => {
    expect(combinePaymentDeposit("1,234.50", "")).toBe("-1234.50");
    expect(combinePaymentDeposit("", "20")).toBe("20.00");
    expect(combinePaymentDeposit("", "")).toBeNull();
    expect(combinePaymentDeposit("x", "")).toBeNull();
    expect(combinePaymentDeposit("1", "2")).toBeNull();
  });
});

describe("dates", () => {
  const today = "2026-09-24";
  it("adds days across month, year, and leap boundaries", () => {
    expect(addDays("2026-01-31", 1)).toBe("2026-02-01");
    expect(addDays("2026-01-01", -1)).toBe("2025-12-31");
    expect(addDays("2024-02-28", 1)).toBe("2024-02-29");
    expect(addDays("2026-02-28", 1)).toBe("2026-03-01");
    expect(addDays("1900-02-28", 1)).toBe("1900-03-01");
  });

  it("parses typed dates", () => {
    expect(parseDate("3/5/2026", today)).toBe("2026-03-05");
    expect(parseDate("3/5/26", today)).toBe("2026-03-05");
    expect(parseDate("3/5", today)).toBe("2026-03-05");
    expect(parseDate("2026-03-05", today)).toBe("2026-03-05");
    expect(parseDate("2/30/2026", today)).toBeNull();
    expect(parseDate("13/1/2026", today)).toBeNull();
    expect(parseDate("junk", today)).toBeNull();
  });

  it("displays MM/DD/YYYY", () => {
    expect(displayDate("2026-03-05")).toBe("03/05/2026");
  });

  it("handles register date keys", () => {
    expect(applyDateKey("+", "2026-03-05", today)).toBe("2026-03-06");
    expect(applyDateKey("-", "2026-03-05", today)).toBe("2026-03-04");
    expect(applyDateKey("t", "2026-03-05", today)).toBe(today);
    expect(applyDateKey("-", "", today)).toBe("2026-09-23");
    expect(applyDateKey("x", "2026-03-05", today)).toBeNull();
  });
});

describe("sanitizeAmountInput", () => {
  it.each([
    ["12a.5x", "12.5"],
    ["-5", "5"],
    ["$1,234.567", "1,234.56"],
    ["1.2.3", "1.23"],
    ["abc", ""],
    ["1,000", "1,000"],
    [".5", ".5"],
  ])("%j → %j", (i, o) => expect(sanitizeAmountInput(i)).toBe(o));
});
