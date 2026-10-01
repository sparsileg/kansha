import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/svelte";
import AccountList from "./AccountList.svelte";
import { listsState } from "../../state/lists.svelte";

beforeEach(() => {
  listsState.accounts = [
    { id: 1, name: "Checking", account_type: "checking", group: "banking", status: "open", show_in_list: true, sort_order: 0 },
    { id: 2, name: "Visa", account_type: "credit_card", group: "credit", status: "open", show_in_list: true, sort_order: 0 },
  ] as never;
  listsState.balances = [
    { account: 1, current: "1200.00", ending: "1200.00" },
    { account: 2, current: "-300.00", ending: "-300.00" },
  ] as never;
  listsState.sectionTotals = { banking: "1200.00", credit: "-300.00" };
});

describe("AccountList section headings", () => {
  it("each heading shows its section's total from Rust at the right", () => {
    render(AccountList);
    const banking = screen.getByRole("heading", { name: /Banking/ });
    expect(within(banking).getByText("1,200.00")).toBeTruthy();
    const credit = screen.getByRole("heading", { name: /Credit/ });
    expect(within(credit).getByText("-300.00")).toBeTruthy();
  });
});
