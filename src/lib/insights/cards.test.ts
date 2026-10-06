import { describe, expect, it } from "vitest";
import { CARDS, cardsNotIn, cardsOf } from "./cards";
import type { SpendingCard } from "../types/bindings";

const ids = (l: { id: string }[]) => l.map((c) => c.id);
const car: SpendingCard = { id: 3, name: "Car", accounts: null, categories: [] };

describe("insight cards (INS-020)", () => {
  it("keeps the stored order; drops unknown and repeated IDs", () => {
    expect(ids(cardsOf(["attention", "bogus", "net_worth", "attention"]))).toEqual(["attention", "net_worth"]);
    expect(cardsOf([])).toEqual([]);
  });

  it("offers the cards not yet on the insight, in catalog order", () => {
    expect(ids(cardsNotIn(["this_month", "attention"]))).toEqual(["net_worth", "net_worth_trend", "upcoming"]);
    expect(ids(cardsNotIn([]))).toEqual(ids([...CARDS]));
  });

  it("spending cards follow the fixed ones, by their names (CARD-060)", () => {
    expect(ids(cardsNotIn(["net_worth"], [car]))).toEqual(["this_month", "net_worth_trend", "upcoming", "attention", "spending:3"]);
    const [c] = cardsOf(["spending:3", "spending:9"], [car]);
    expect(c).toEqual({ id: "spending:3", label: "Car", double: true, spending: 3 });
    expect(cardsOf(["spending:3"])).toEqual([]); // deleted: skipped
  });
});
