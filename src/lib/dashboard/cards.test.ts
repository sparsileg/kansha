import { describe, expect, it } from "vitest";
import { CARDS, cardsNotIn, cardsOf } from "./cards";

const ids = (l: { id: string }[]) => l.map((c) => c.id);

describe("insight cards (INS-020)", () => {
  it("keeps the stored order; drops unknown and repeated IDs", () => {
    expect(ids(cardsOf(["attention", "bogus", "net_worth", "attention"]))).toEqual(["attention", "net_worth"]);
    expect(cardsOf([])).toEqual([]);
  });

  it("offers the cards not yet on the insight, in catalog order", () => {
    expect(ids(cardsNotIn(["this_month", "attention"]))).toEqual(["net_worth", "net_worth_trend", "upcoming"]);
    expect(ids(cardsNotIn([]))).toEqual(ids([...CARDS]));
  });
});
