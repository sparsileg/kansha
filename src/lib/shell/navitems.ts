// What the navigation bar can hold, and the pure list operations the
// "Navigation Bar" dialog uses. An item is a menu action, Home, or an
// account: everything that can be run has an id (see actions.ts). No I/O.

import type { Account } from "../types/bindings";
import { MENUS } from "./menus";

export interface NavEntry {
  id: string;
  /** What the button says. */
  label: string;
  /** Heading in the dialog's list of everything available. */
  group: string;
  /** Present when it cannot be used yet; says why. */
  disabled?: string;
}

export const HOME_ID = "home";
export const INVESTMENTS_ID = "view.investments";
export const DEFAULT_NAV = [HOME_ID, "tools.reminders", "tools.calendar", "tools.reconcile", INVESTMENTS_ID];

/** Button text where the menu wording alone would be unclear on a bar. */
const NAV_LABEL: Record<string, string> = {
  "file.new": "New book",
  "file.open": "Open book",
  "file.backup": "Backup",
  "file.restore": "Restore",
  "file.import": "Import",
  "file.export": "Export",
  "edit.navbar": "Navigation bar",
  "reports.saved": "Saved reports",
  "reports.investing": "Investing report",
  "reports.balances": "Balances report",
  "reports.spending": "Spending report",
  "reports.taxes": "Tax report",
  "help.about": "About",
};

const plain = (label: string) => label.replace(/…$/, "");

/** Everything that can go on the bar: Home, the investments view, every
 * menu item, and every account. */
export function navCatalog(accounts: Account[]): NavEntry[] {
  const out: NavEntry[] = [
    { id: HOME_ID, label: "Home", group: "General" },
    { id: INVESTMENTS_ID, label: "Investments", group: "General", disabled: "Planned: Phase 6" },
  ];
  for (const menu of MENUS) {
    for (const item of menu.items) {
      out.push({
        id: item.id,
        label: NAV_LABEL[item.id] ?? plain(item.label),
        group: menu.label,
        disabled: item.disabled,
      });
    }
  }
  for (const a of accounts) {
    out.push({ id: `account:${a.id}`, label: a.name, group: "Accounts" });
  }
  return out;
}

/** The entries for `ids`, in that order. Ids that no longer exist (a
 * deleted account) and repeats are skipped. */
export function resolveNav(ids: string[], catalog: NavEntry[]): NavEntry[] {
  const byId = new Map(catalog.map((e) => [e.id, e]));
  const seen = new Set<string>();
  const out: NavEntry[] = [];
  for (const id of ids) {
    const e = byId.get(id);
    if (e && !seen.has(id)) {
      seen.add(id);
      out.push(e);
    }
  }
  return out;
}

export function addNav(ids: string[], id: string): string[] {
  return ids.includes(id) ? ids : [...ids, id];
}

export function removeNav(ids: string[], id: string): string[] {
  return ids.filter((x) => x !== id);
}

/** Move `id` one place earlier (`-1`) or later (`1`); unchanged at an end. */
export function moveNav(ids: string[], id: string, delta: -1 | 1): string[] {
  const from = ids.indexOf(id);
  const to = from + delta;
  if (from < 0 || to < 0 || to >= ids.length) return ids;
  const out = [...ids];
  [out[from], out[to]] = [out[to], out[from]];
  return out;
}

/** A saved list back into ids, or null if it is not a list of strings. */
export function parseNav(json: string): string[] | null {
  try {
    const v: unknown = JSON.parse(json);
    return Array.isArray(v) && v.every((x) => typeof x === "string") ? (v as string[]) : null;
  } catch {
    return null;
  }
}
