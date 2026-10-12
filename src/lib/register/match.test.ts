import { describe, expect, it } from "vitest";
import { completeSegment, matchTargets, newCategoryPath, type TargetOption } from "./match";

const opts: TargetOption[] = [
  { value: "split", label: "--Split--" },
  { value: "c:1", label: "Food:Groceries" },
  { value: "c:2", label: "Auto:Fuel" },
  { value: "c:3", label: "Bank Charges" },
  { value: "a:1", label: "[Checking]" },
  { value: "a:2", label: "[Savings]" },
  { value: "c:4", label: "Income:Check Deposit" },
];
const values = (q: string) => matchTargets(opts, q).map((o) => o.value);

describe("matchTargets", () => {
  it("finds an account without typing brackets", () => {
    expect(values("checking")).toEqual(["a:1"]);
    expect(values("CHECK")).toEqual(["a:1", "c:4"]);
  });
  it("matches categories by any part of the path", () => {
    expect(values("groc")).toEqual(["c:1"]);
    expect(values("fuel")).toEqual(["c:2"]);
    expect(values("ing")).toEqual(["a:1", "a:2"]);
  });
  it("ranks prefix, then word start, then contains", () => {
    const s = values("s");
    expect(s[0]).toBe("a:2"); // label starts with "s"
    expect(s.indexOf("split")).toBeLessThan(s.indexOf("c:1")); // word start before contains
    expect(values("dep")).toEqual(["c:4"]);
  });
  it("requires every word", () => {
    expect(values("check dep")).toEqual(["c:4"]);
    expect(values("zzz")).toEqual([]);
  });
  it("returns everything for an empty query", () => {
    expect(values("")).toHaveLength(opts.length);
  });
});

describe("newCategoryPath", () => {
  it("offers a new single-level or nested path, trimmed", () => {
    expect(newCategoryPath(opts, "Fuel")).toBe("Fuel");
    expect(newCategoryPath(opts, " Charity : Fast Offering ")).toBe("Charity:Fast Offering");
    expect(newCategoryPath(opts, "Auto:Tires")).toBe("Auto:Tires");
  });
  it("offers nothing for existing paths, accounts, or incomplete text", () => {
    expect(newCategoryPath(opts, "")).toBeNull();
    expect(newCategoryPath(opts, "auto:fuel")).toBeNull();
    expect(newCategoryPath(opts, "Bank Charges")).toBeNull();
    expect(newCategoryPath(opts, "checking")).toBeNull(); // an account, brackets are display only
    expect(newCategoryPath(opts, "[Checking]")).toBeNull();
    expect(newCategoryPath(opts, "Charity:")).toBeNull();
    expect(newCategoryPath(opts, "A::B")).toBeNull();
    expect(newCategoryPath(opts, "--Split--")).toBeNull();
  });
});

describe("completeSegment", () => {
  const tree: TargetOption[] = [
    { value: "c:1", label: "Car" },
    { value: "c:2", label: "Car:BlueForester" },
    { value: "c:3", label: "Car:BlueForester:Gas" },
    { value: "c:4", label: "Car:BlueForester:Repairs" },
    { value: "c:5", label: "Charity" },
    { value: "c:6", label: "Food" },
    { value: "c:7", label: "Food:Groceries" },
    { value: "c:8", label: "Food:Dining" },
    { value: "c:9", label: "Fuel" },
    { value: "a:1", label: "[Checking]" },
    { value: "split", label: "--Split--" },
  ];
  const done = (t: string) => completeSegment(tree, t);

  it("fills in the one child that starts with the typed part", () => {
    expect(done("Car:B")).toEqual({ text: "Car:BlueForester", value: "c:2" });
    expect(done("Car:BlueForester:G")).toEqual({ text: "Car:BlueForester:Gas", value: "c:3" });
  });
  it("works at the top level and ignores accounts and split", () => {
    expect(done("Ca")).toEqual({ text: "Car", value: "c:1" });
    expect(done("Ch")).toEqual({ text: "Charity", value: "c:5" });
    expect(done("-")).toBeNull();
    expect(done("[Ch")).toBeNull();
  });
  it("keeps the typed case and spacing; the rest comes as stored", () => {
    expect(done("car : b")).toEqual({ text: "car : blueForester", value: "c:2" });
  });
  it("completes nothing when two names fit, none fits, or the name is whole", () => {
    expect(done("C")).toBeNull(); // Car, Charity
    expect(done("F")).toBeNull(); // Food, Fuel
    expect(done("Food:")).toBeNull(); // nothing typed yet
    expect(done("Food:X")).toBeNull();
    expect(done("Fuel")).toBeNull();
    expect(done("Zoo:G")).toBeNull(); // no such parent
    expect(done("Car::G")).toBeNull();
    expect(done("")).toBeNull();
  });
  it("counts a name once however many paths go through it", () => {
    expect(done("Car:BlueForester:R")).toEqual({ text: "Car:BlueForester:Repairs", value: "c:4" });
    expect(done("Car:Bl")).toEqual({ text: "Car:BlueForester", value: "c:2" });
  });
});
