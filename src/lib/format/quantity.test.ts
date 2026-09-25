import { describe, expect, it } from "vitest";
import { formatPrice, formatQuantity, parseScaled, sanitizeScaledInput } from "./quantity";

describe("parseScaled (shares and prices)", () => {
  it.each([
    ["10", "10"],
    ["1,000.5", "1000.5"],
    [".25", "0.25"],
    ["$41.250", "41.25"],
    ["0.333333", "0.333333"],
    ["007.10", "7.1"],
    ["5.", "5"],
  ])("%s → %s", (i, o) => expect(parseScaled(i)).toBe(o));

  it.each(["", ".", "-1", "1.2345678", "abc", "1 2", "+3"])("rejects %j", (i) =>
    expect(parseScaled(i)).toBeNull(),
  );
});

describe("display", () => {
  it("groups share thousands and keeps their decimals", () => {
    expect(formatQuantity("1234.5")).toBe("1,234.5");
    expect(formatQuantity("0.666667")).toBe("0.666667");
    expect(formatQuantity("-15")).toBe("-15");
  });

  it("shows prices with at least two decimals", () => {
    expect(formatPrice("320")).toBe("320.00");
    expect(formatPrice("41.2")).toBe("41.20");
    expect(formatPrice("149.999925")).toBe("149.999925");
    expect(formatPrice("1234.5")).toBe("1,234.50");
  });

  it("sanitizes typing to digits, commas, one point, six decimals", () => {
    expect(sanitizeScaledInput("1a,2.3.4567890")).toBe("1,2.345678");
  });
});
