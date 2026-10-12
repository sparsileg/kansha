import { describe, expect, it } from "vitest";
import type { AccountType } from "../types/bindings";
import { amountLabels } from "./columns";

const acct = (account_type: AccountType, investment: object | null = null) => ({ account_type, investment }) as never;
const pair = (t: AccountType, investment: object | null = null) => {
  const l = amountLabels(acct(t, investment));
  return [l.out, l.in, l.outs, l.ins];
};

describe("amountLabels", () => {
  it("names each account type's two directions as Quicken does", () => {
    expect(pair("checking")).toEqual(["Payment", "Deposit", "payments", "deposits"]);
    expect(pair("savings")).toEqual(["Payment", "Deposit", "payments", "deposits"]);
    expect(pair("money_market")).toEqual(["Payment", "Deposit", "payments", "deposits"]);
    expect(pair("cash")).toEqual(["Spend", "Receive", "amounts spent", "amounts received"]);
    expect(pair("credit_card")).toEqual(["Charge", "Payment", "charges", "payments"]);
    expect(pair("loan")).toEqual(["Increase", "Decrease", "increases", "decreases"]);
    expect(pair("other_liability")).toEqual(["Increase", "Decrease", "increases", "decreases"]);
    expect(pair("other_asset")).toEqual(["Decrease", "Increase", "decreases", "increases"]);
    expect(pair("brokerage", {})).toEqual(["Cash out", "Cash in", "cash outs", "cash ins"]);
  });
  it("is Payment and Deposit with no account", () => {
    expect(amountLabels(undefined)).toMatchObject({ out: "Payment", in: "Deposit" });
  });
});
