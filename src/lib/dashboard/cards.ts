// The dashboard's cards (DSH-010 … DSH-040). Each has a stable ID. The user
// chooses which show and in what order (DSH-040); the choice is kept in the
// book (`dashboard_cards`) as a `CardLayout`.
//
// To add a card: add its ID to `CardId`, its entry to `CARDS` (the default
// order), and its body to `bodies` in `views/Dashboard.svelte`. A card the
// stored layout does not know shows, after the others, until the user hides it.

export type CardId = "net_worth" | "this_month" | "net_worth_trend" | "upcoming" | "attention";

export interface CardDef {
  id: CardId;
  /** The card's name where cards are chosen; its heading may say more. */
  label: string;
  /** Spans the dashboard's full width. */
  wide?: boolean;
  /** Two columns wide, when there is room for two. */
  double?: boolean;
}

/** Every card, in the default order. */
export const CARDS: readonly CardDef[] = [
  { id: "net_worth", label: "Net worth" },
  { id: "this_month", label: "This month" },
  { id: "net_worth_trend", label: "Net worth, last 12 months", wide: true },
  { id: "upcoming", label: "Due soon", double: true },
  { id: "attention", label: "Needs attention" },
];

/** Which cards show, and their order. */
export interface CardLayout {
  /** Card IDs, top to bottom, left to right; hidden ones keep their place. */
  order: CardId[];
  hidden: CardId[];
}

const known = new Set<string>(CARDS.map((c) => c.id));
const isCardId = (v: unknown): v is CardId => typeof v === "string" && known.has(v);

export const defaultLayout = (): CardLayout => ({ order: CARDS.map((c) => c.id), hidden: [] });

/** Drop unknown and repeated IDs; put cards the list lacks last. */
function complete(order: unknown[]): CardId[] {
  const seen = [...new Set(order.filter(isCardId))];
  return [...seen, ...CARDS.map((c) => c.id).filter((id) => !seen.includes(id))];
}

/** Stored text back into a layout; anything unreadable gives the default. */
export function parseLayout(text: string | null): CardLayout {
  if (!text) return defaultLayout();
  try {
    const raw: unknown = JSON.parse(text);
    if (typeof raw !== "object" || raw === null) return defaultLayout();
    const o = raw as Record<string, unknown>;
    const listed = Array.isArray(o.order) ? o.order : [];
    const hidden = Array.isArray(o.hidden) ? [...new Set(o.hidden.filter(isCardId))] : [];
    return { order: complete(listed), hidden };
  } catch {
    return defaultLayout();
  }
}

export const serializeLayout = (l: CardLayout): string => JSON.stringify(l);

/** The layout to store: `null` for the default, so a later release's
 * default applies. */
export function storedLayout(l: CardLayout): string | null {
  const d = defaultLayout();
  const same = l.hidden.length === 0 && l.order.every((id, i) => id === d.order[i]);
  return same ? null : serializeLayout(l);
}

const byId = (id: CardId): CardDef => CARDS.find((c) => c.id === id) as CardDef;

/** Every card in the layout's order, hidden ones too (the chooser). */
export const orderedCards = (l: CardLayout): CardDef[] => l.order.map(byId);

/** The cards to show, in order. */
export const shownCards = (l: CardLayout): CardDef[] =>
  l.order.filter((id) => !l.hidden.includes(id)).map(byId);

/** Show or hide one card. */
export const setCardShown = (l: CardLayout, id: CardId, shown: boolean): CardLayout => ({
  order: l.order,
  hidden: shown ? l.hidden.filter((h) => h !== id) : l.hidden.includes(id) ? l.hidden : [...l.hidden, id],
});
