// Account list grouping (ACCT-240). Display order only; no money math.
//
// Accounts belong to groups (stored in the book). The account list shows
// them in six sections; Assets & Debt shows two groups, Assets and
// Liabilities, which the Net Worth report keeps apart.

import type { Account, AccountGroup, AccountType, GroupOrder } from "../types/bindings";

export const GROUP_ORDER: AccountGroup[] = [
  "banking",
  "credit",
  "investments",
  "retirement",
  "assets",
  "liabilities",
  "other",
];

export const GROUP_LABEL: Record<AccountGroup, string> = {
  banking: "Banking",
  credit: "Credit",
  investments: "Investments",
  retirement: "Retirement",
  assets: "Assets",
  liabilities: "Liabilities",
  other: "Other",
};

export type SectionId = "banking" | "credit" | "investments" | "retirement" | "assets_debt" | "other";

export interface Section {
  id: SectionId;
  label: string;
  groups: AccountGroup[];
}

/** The account list's sections, in order. */
export const SECTIONS: Section[] = [
  { id: "banking", label: "Banking", groups: ["banking"] },
  { id: "credit", label: "Credit", groups: ["credit"] },
  { id: "investments", label: "Investments", groups: ["investments"] },
  { id: "retirement", label: "Retirement", groups: ["retirement"] },
  { id: "assets_debt", label: "Assets & Debt", groups: ["assets", "liabilities"] },
  { id: "other", label: "Other", groups: ["other"] },
];

export const sectionOf = (g: AccountGroup): Section =>
  SECTIONS.find((s) => s.groups.includes(g)) ?? SECTIONS[SECTIONS.length - 1];

export interface AccountGroupView {
  section: SectionId;
  label: string;
  accounts: Account[];
}

const LIABILITY_TYPES = new Set<AccountType>(["credit_card", "other_liability", "loan"]);

/** The group an account takes in a section: its own when the section has
 * it; in Assets & Debt, Liabilities for a debt and Assets otherwise. */
export function groupIn(section: Section, a: Account): AccountGroup {
  if (section.groups.includes(a.group)) return a.group;
  if (section.groups.length === 1) return section.groups[0];
  return LIABILITY_TYPES.has(a.account_type) ? "liabilities" : "assets";
}

/**
 * Accounts by section, in section order; inside a section, by
 * `sort_order` then name. Closed accounts and those hidden from the list
 * (`show_in_list` false) are left out unless `includeClosed`. Empty
 * sections are dropped unless `keepEmpty`.
 */
export function groupAccounts(
  accounts: Account[],
  includeClosed: boolean,
  keepEmpty = false,
): AccountGroupView[] {
  return SECTIONS.map((s) => ({
    section: s.id,
    label: s.label,
    accounts: accounts
      .filter(
        (a) =>
          s.groups.includes(a.group) &&
          (includeClosed || (a.status === "open" && a.show_in_list)),
      )
      .sort(
        (x, y) => x.sort_order - y.sort_order || x.name.localeCompare(y.name),
      ),
  })).filter((g) => keepEmpty || g.accounts.length > 0);
}

/**
 * The arrangement to store: each section's accounts in list order, as
 * runs of one group each (Assets & Debt mixes two), so every account
 * keeps its place.
 */
export function arrangement(sections: { section: SectionId; accounts: Account[] }[]): GroupOrder[] {
  const out: GroupOrder[] = [];
  for (const s of sections) {
    const def = SECTIONS.find((x) => x.id === s.section);
    if (!def) continue;
    // A run never reaches back into the section before.
    const start = out.length;
    for (const a of s.accounts) {
      const group = groupIn(def, a);
      const run = out.length > start ? out[out.length - 1] : undefined;
      if (run?.group === group) run.accounts.push(a.id);
      else out.push({ group, accounts: [a.id] });
    }
  }
  return out;
}
