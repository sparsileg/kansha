// The cards an insight can show (INS-020, CARD-010 … CARD-060). Each has a
// stable ID; an insight stores the IDs of its cards, in order, in the
// book (`insight` table). One card can be on several insights.
//
// The fixed cards are listed here; spending cards (CARD-060) are made by
// the user, kept in the book, and shown by the ID `spending:<id>`.
//
// To add a fixed card: add its ID to `CardId`, its entry to `CARDS`, and
// its body to `bodies` in `views/Insights.svelte`.

import type { SpendingCard, SpendingCardId } from "../types/bindings";

export type CardId =
  "net_worth" | "this_month" | "net_worth_trend" | "upcoming" | "attention";

export interface CardDef {
  /** A `CardId`, or `spending:<id>`. */
  id: string;
  /** The card's name where cards are chosen; its heading may say more. */
  label: string;
  /** Spans the insight's full width. */
  wide?: boolean;
  /** Two columns wide, when there is room for two. */
  double?: boolean;
  /** Set on a spending card. */
  spending?: SpendingCardId;
}

/** The fixed cards, in the order the chooser lists them. */
export const CARDS: readonly CardDef[] = [
  { id: "net_worth", label: "Net worth", double: true },
  { id: "this_month", label: "This month" },
  { id: "net_worth_trend", label: "Net worth over time", wide: true },
  { id: "upcoming", label: "Due soon", double: true },
  { id: "attention", label: "Needs attention" },
];

/** The card ID an insight stores for a spending card. */
export const spendingId = (id: SpendingCardId): string => `spending:${id}`;

/** Every card: the fixed ones, then the spending cards (by name, as
 * Rust lists them). */
export const catalog = (spending: readonly SpendingCard[]): CardDef[] => [
  ...CARDS,
  ...spending.map((s) => ({
    id: spendingId(s.id),
    label: s.name,
    double: true,
    spending: s.id,
  })),
];

/** The cards for stored IDs, in order; unknown IDs (from a later
 * release, or a deleted card) and repeats are skipped. */
export function cardsOf(
  ids: readonly string[],
  spending: readonly SpendingCard[] = [],
): CardDef[] {
  const byId = new Map(catalog(spending).map((c) => [c.id, c]));
  const out: CardDef[] = [];
  for (const id of ids) {
    const c = byId.get(id);
    if (c && !out.includes(c)) out.push(c);
  }
  return out;
}

/** The cards not in `ids`, in catalog order: what can still be added. */
export const cardsNotIn = (
  ids: readonly string[],
  spending: readonly SpendingCard[] = [],
): CardDef[] => catalog(spending).filter((c) => !ids.includes(c.id));
