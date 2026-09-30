// Panels: views that open as single windows, so they can wait in the dock
// (UI-040). Each kind has one window at most.

import { windowState } from "../state/windows.svelte";

export type PanelKind = "calendar" | "scheduled" | "accounts" | "reconcile" | "investments";

export const PANELS: Record<PanelKind, string> = {
  calendar: "Calendar",
  scheduled: "Reminders",
  accounts: "Accounts",
  reconcile: "Reconcile",
  investments: "Investments",
};

export function openPanel(kind: PanelKind): void {
  windowState.openSingle(kind, PANELS[kind]);
}

export const isPanel = (kind: string | null): kind is PanelKind => kind !== null && kind in PANELS;
