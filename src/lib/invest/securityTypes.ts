// Security types as the UI names them (SEC-010).

import type { SecurityType } from "../types/bindings";

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
