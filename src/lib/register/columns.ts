// The two directions of an account's money: the register's amount
// columns and a schedule's transaction type, named as in Quicken.
// Payment (money out) and Deposit (money in); a cash account's Spend and
// Receive; a credit card's Charge and Payment; a loan's or other
// liability's Increase and Decrease; an other asset's Decrease and
// Increase; an investment account's cash, Cash out and Cash in. `outs`
// and `ins` name several, as in "Average of past charges".

import type { Account } from "../types/bindings";

export type AmountLabels = { out: string; in: string; outs: string; ins: string };

const label = (out: string, in_: string, outs: string, ins: string): AmountLabels => ({ out, in: in_, outs, ins });

export function amountLabels(account: Pick<Account, "account_type" | "investment"> | undefined): AmountLabels {
  switch (account?.account_type) {
    case "cash":
      return label("Spend", "Receive", "amounts spent", "amounts received");
    case "credit_card":
      return label("Charge", "Payment", "charges", "payments");
    case "loan":
    case "other_liability":
      return label("Increase", "Decrease", "increases", "decreases");
    case "other_asset":
      return label("Decrease", "Increase", "decreases", "increases");
  }
  if (account?.investment != null) return label("Cash out", "Cash in", "cash outs", "cash ins");
  return label("Payment", "Deposit", "payments", "deposits");
}
