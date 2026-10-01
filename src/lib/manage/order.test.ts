import { describe, expect, it } from "vitest";
import { hiddenLast } from "./order";

describe("hiddenLast", () => {
  it("lists visible rows alphabetically, then hidden ones alphabetically", () => {
    const rows = [
      { n: "pear", hidden: true },
      { n: "Zebra", hidden: false },
      { n: "apple", hidden: true },
      { n: "banana", hidden: false },
    ];
    expect(hiddenLast(rows, (r) => r.n).map((r) => r.n)).toEqual(["banana", "Zebra", "apple", "pear"]);
  });

  it("leaves its input alone", () => {
    const rows = [{ n: "b", hidden: false }, { n: "a", hidden: false }];
    hiddenLast(rows, (r) => r.n);
    expect(rows[0].n).toBe("b");
  });
});
