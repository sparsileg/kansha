// The cards an insight can show (INS-020, DSH-010 … DSH-030). Each has a
// stable ID; an insight stores the IDs of its cards, in order, in the
// book (`insight` table). One card can be on several insights.
//
// To add a card: add its ID to `CardId`, its entry to `CARDS`, and its
// body to `bodies` in `views/Insights.svelte`.

export type CardId = "net_worth" | "this_month" | "net_worth_trend" | "upcoming" | "attention";

export interface CardDef {
  id: CardId;
  /** The card's name where cards are chosen; its heading may say more. */
  label: string;
  /** Spans the insight's full width. */
  wide?: boolean;
  /** Two columns wide, when there is room for two. */
  double?: boolean;
}

/** Every card, in the order the chooser lists them. */
export const CARDS: readonly CardDef[] = [
  { id: "net_worth", label: "Net worth" },
  { id: "this_month", label: "This month" },
  { id: "net_worth_trend", label: "Net worth, last 12 months", wide: true },
  { id: "upcoming", label: "Due soon", double: true },
  { id: "attention", label: "Needs attention" },
];

const byId = new Map<string, CardDef>(CARDS.map((c) => [c.id, c]));

/** The cards for stored IDs, in order; unknown IDs (from a later
 * release) and repeats are skipped. */
export function cardsOf(ids: readonly string[]): CardDef[] {
  const out: CardDef[] = [];
  for (const id of ids) {
    const c = byId.get(id);
    if (c && !out.includes(c)) out.push(c);
  }
  return out;
}

/** The cards not in `ids`, in catalog order: what can still be added. */
export const cardsNotIn = (ids: readonly string[]): CardDef[] => CARDS.filter((c) => !ids.includes(c.id));
