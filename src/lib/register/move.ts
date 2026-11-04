import type { Account, AccountType } from "../types/bindings";

/** Account types a transaction moves between (REG-085). The engine has
 * the same rule (`ledger::is_movable_type`) and refuses any other. */
const MOVABLE = new Set<AccountType>(["checking", "savings", "cash", "credit_card"]);

export const isMovable = (a: Pick<Account, "account_type"> | undefined): boolean =>
  a !== undefined && MOVABLE.has(a.account_type);

/** The Move dialog's accounts: open movable accounts other than `from`,
 * by name. */
export function moveTargets(accounts: Account[], from: number): Account[] {
  return accounts
    .filter((a) => a.id !== from && a.status === "open" && isMovable(a))
    .sort((a, b) => a.name.localeCompare(b.name));
}
