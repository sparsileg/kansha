// Security types as the UI names them (SEC-010).

import type { AssetClass, ClassShare, SecurityType } from "../types/bindings";

export const SECURITY_TYPES: [SecurityType, string][] = [
  ["stock", "Stock"],
  ["etf", "ETF"],
  ["mutual_fund", "Mutual fund"],
  ["bond", "Bond"],
  ["money_market", "Money market fund"],
  ["cd", "CD"],
  ["other", "Other"],
];

export const securityTypeLabel = (t: SecurityType): string =>
  SECURITY_TYPES.find(([k]) => k === t)?.[1] ?? t;

/** Asset classes as the UI names them, in Kansha's order (SEC-010). */
export const ASSET_CLASSES: [AssetClass, string][] = [
  ["us_equity", "US equity"],
  ["us_small_mid", "US small/mid"],
  ["intl_equity", "Intl equity"],
  ["emerging", "Emerging mkts"],
  ["bond_short", "Short-term bonds"],
  ["bond_intermediate", "Intermed bonds"],
  ["bond_long", "Long-term bonds"],
  ["bond_intl", "Intl bonds"],
  ["bond_tips", "TIPS"],
  ["bond_muni", "Muni bonds"],
  ["cash", "Cash"],
  ["real_estate", "Real estate"],
  ["commodity", "Commodities"],
  ["other", "Other"],
];

export const assetClassLabel = (c: AssetClass): string =>
  ASSET_CLASSES.find(([k]) => k === c)?.[1] ?? c;

/** How a list shows a security's class: its mix (SEC-050) or its class. */
export const classText = (s: { asset_class: AssetClass; class_mix: ClassShare[] }): string =>
  s.class_mix.length > 0
    ? s.class_mix.map((c) => `${assetClassLabel(c.asset_class)} ${c.percent}%`).join(", ")
    : assetClassLabel(s.asset_class);
