import { describe, expect, it } from "vitest";
import {
  CARDS,
  defaultLayout,
  orderedCards,
  parseLayout,
  serializeLayout,
  setCardShown,
  shownCards,
  storedLayout,
} from "./cards";

const ids = (l: { id: string }[]) => l.map((c) => c.id);

describe("dashboard card layout (DSH-040)", () => {
  it("nothing stored, or unreadable text, gives every card in the default order", () => {
    for (const t of [null, "", "not json", "5", "null"]) {
      expect(ids(shownCards(parseLayout(t)))).toEqual(ids([...CARDS]));
    }
  });

  it("keeps the stored order and hidden cards", () => {
    const l = parseLayout(JSON.stringify({ order: ["upcoming", "net_worth"], hidden: ["net_worth"] }));
    expect(ids(orderedCards(l)).slice(0, 2)).toEqual(["upcoming", "net_worth"]);
    expect(ids(shownCards(l))).toEqual(["upcoming", "this_month", "net_worth_trend", "attention"]);
  });

  it("drops unknown and repeated IDs; a card the list lacks shows last", () => {
    const l = parseLayout(JSON.stringify({ order: ["bogus", "attention", "attention"], hidden: ["bogus", 3] }));
    expect(l.order).toEqual(["attention", "net_worth", "this_month", "net_worth_trend", "upcoming"]);
    expect(l.hidden).toEqual([]);
  });

  it("round-trips, and the default is stored as nothing", () => {
    const l = setCardShown(defaultLayout(), "upcoming", false);
    expect(parseLayout(serializeLayout(l))).toEqual(l);
    expect(storedLayout(l)).toBe(serializeLayout(l));
    expect(storedLayout(defaultLayout())).toBeNull();
    expect(storedLayout(setCardShown(l, "upcoming", true))).toBeNull();
  });
});
