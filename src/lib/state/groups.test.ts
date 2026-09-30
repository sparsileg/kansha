import { describe, expect, it } from "vitest";
import { arrangement, groupAccounts, SECTIONS } from "./groups";
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
    expect(g.map((x) => x.section)).toEqual(["banking", "credit"]);
    expect(g[0].accounts.map((a) => a.name)).toEqual(["First", "Beta", "Zeta"]);
  });

  it("includes closed and hidden on request", () => {
    const g = groupAccounts(list, true);
    expect(g[0].accounts.map((a) => a.name)).toContain("Old");
    expect(g[0].accounts.map((a) => a.name)).toContain("Hidden");
  });

  it("shows six sections; Assets & Debt holds assets and liabilities; Other holds HSAs", () => {
    expect(SECTIONS.map((x) => x.label)).toEqual([
      "Banking", "Credit", "Investments", "Retirement", "Assets & Debt", "Other",
    ]);
    const g = groupAccounts(
      [
        acct({ id: 1, name: "House", group: "assets", sort_order: 1 }),
        acct({ id: 2, name: "Mortgage", group: "liabilities", sort_order: 0 }),
        acct({ id: 3, name: "HSA", group: "other" }),
      ],
      false,
    );
    expect(g.map((x) => [x.label, x.accounts.map((a) => a.name)])).toEqual([
      ["Assets & Debt", ["Mortgage", "House"]],
      ["Other", ["HSA"]],
    ]);
  });

  it("arranges each section as runs of one group, keeping the order", () => {
    const house = acct({ id: 1, name: "House", group: "assets", account_type: "other_asset" });
    const loan = acct({ id: 2, name: "Loan", group: "liabilities", account_type: "loan" });
    const car = acct({ id: 3, name: "Car", group: "assets", account_type: "other_asset" });
    const hsa = acct({ id: 4, name: "HSA", group: "retirement", account_type: "hsa" });
    const card = acct({ id: 5, name: "Card", group: "credit", account_type: "credit_card" });
    expect(
      arrangement([
        { section: "assets_debt", accounts: [house, loan, car, card] },
        { section: "other", accounts: [hsa] },
      ]),
    ).toEqual([
      { group: "assets", accounts: [1] },
      { group: "liabilities", accounts: [2] },
      { group: "assets", accounts: [3] },
      // A credit card moved into Assets & Debt is a debt.
      { group: "liabilities", accounts: [5] },
      { group: "other", accounts: [4] },
    ]);
  });
});
