import { describe, expect, it } from "vitest";
import { groupAccounts } from "./groups";
import type { Account } from "../types/bindings";

const acct = (o: Partial<Account>): Account =>
  ({
    id: 1,
    name: "A",
    group: "banking",
    status: "open",
    show_in_list: true,
    sort_order: 0,
    ...o,
  }) as Account;

describe("groupAccounts", () => {
  const list = [
    acct({ id: 1, name: "Visa", group: "credit" }),
    acct({ id: 2, name: "Zeta", sort_order: 1 }),
    acct({ id: 3, name: "Beta", sort_order: 1 }),
    acct({ id: 4, name: "First", sort_order: 0 }),
    acct({ id: 5, name: "Old", status: "closed" }),
    acct({ id: 6, name: "Hidden", show_in_list: false }),
  ];

  it("orders groups, then sort_order, then name; hides closed", () => {
    const g = groupAccounts(list, false);
    expect(g.map((x) => x.group)).toEqual(["banking", "credit"]);
    expect(g[0].accounts.map((a) => a.name)).toEqual(["First", "Beta", "Zeta"]);
  });

  it("includes closed and hidden on request", () => {
    const g = groupAccounts(list, true);
    expect(g[0].accounts.map((a) => a.name)).toContain("Old");
    expect(g[0].accounts.map((a) => a.name)).toContain("Hidden");
  });
});
