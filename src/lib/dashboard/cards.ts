// The dashboard's cards (DSH-010 … DSH-030). Each has a stable ID, so a
// later setting can choose which cards show and in what order (DSH-040);
// until then every card shows, in this order.

export type CardId = "net_worth" | "this_month" | "net_worth_trend" | "upcoming" | "attention";

export interface CardDef {
  id: CardId;
  /** The card's name where cards are chosen; its heading may say more. */
  label: string;
  /** Spans the dashboard's full width. */
  wide?: boolean;
}

export const CARDS: readonly CardDef[] = [
  { id: "net_worth", label: "Net worth" },
  { id: "this_month", label: "This month" },
  { id: "net_worth_trend", label: "Net worth, last 12 months", wide: true },
  { id: "upcoming", label: "Due soon" },
  { id: "attention", label: "Needs attention" },
];
