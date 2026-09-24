// Account list grouping (ACCT-240). Display order only; no money math.

import type { Account, AccountGroup } from "../types/bindings";

export const GROUP_ORDER: AccountGroup[] = [
  "banking",
  "credit",
  "investments",
  "retirement",
  "assets",
  "liabilities",
];

export const GROUP_LABEL: Record<AccountGroup, string> = {
  banking: "Banking",
  credit: "Credit",
  investments: "Investments",
  retirement: "Retirement",
  assets: "Assets",
  liabilities: "Liabilities",
};

export interface AccountGroupView {
  group: AccountGroup;
  label: string;
  accounts: Account[];
}

/**
 * Group accounts in the fixed group order; inside a group, by `sort_order`
 * then name. Closed accounts and those hidden from the list
 * (`show_in_list` false) are left out unless `includeClosed`. Empty groups
 * are dropped.
 */
export function groupAccounts(
  accounts: Account[],
  includeClosed: boolean,
): AccountGroupView[] {
  return GROUP_ORDER.map((group) => ({
    group,
    label: GROUP_LABEL[group],
    accounts: accounts
      .filter(
        (a) =>
          a.group === group &&
          (includeClosed || (a.status === "open" && a.show_in_list)),
      )
      .sort(
        (x, y) => x.sort_order - y.sort_order || x.name.localeCompare(y.name),
      ),
  })).filter((g) => g.accounts.length > 0);
}
