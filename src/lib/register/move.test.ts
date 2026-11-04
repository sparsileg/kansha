import { describe, expect, it } from "vitest";
import type { Account, AccountType } from "../types/bindings";
import { isMovable, moveTargets } from "./move";

const acct = (id: number, name: string, account_type: AccountType, status: "open" | "closed" = "open") =>
  ({ id, name, account_type, status }) as Account;

describe("moveTargets (REG-085)", () => {
  const all = [
    acct(1, "Visa", "credit_card"),
    acct(2, "Amex", "credit_card"),
    acct(3, "Checking", "checking"),
    acct(4, "Wallet", "cash"),
    acct(5, "Savings", "savings"),
    acct(6, "Old card", "credit_card", "closed"),
    acct(7, "Car loan", "loan"),
    acct(8, "Brokerage", "brokerage"),
    acct(9, "Money market", "money_market"),
    acct(10, "House", "other_asset"),
  ];

  it("lists open checking, savings, cash, and card accounts but this one, by name", () => {
    expect(moveTargets(all, 1).map((a) => a.name)).toEqual(["Amex", "Checking", "Savings", "Wallet"]);
  });

  it("offers Move only in those registers", () => {
    expect(all.filter(isMovable).map((a) => a.id)).toEqual([1, 2, 3, 4, 5, 6]);
    expect(isMovable(undefined)).toBe(false);
  });
});
