import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });
const brokerage = {
  name: "", account_type: "brokerage", group: "investments", tax_treatment: "taxable", description: "", institution: "",
  account_number: "", contact_phone: "", home_url: "", notes: "", opening_date: null, show_in_bar: true, show_in_list: true,
  sort_order: 0, interest_rate: null, credit_limit: null,
  investment: { subtype: null, cash_mode: "internal", linked_cash_account: null, mmf_mode: "cash", default_lot_method: "hifo" },
  other_asset: null, tax_line_out: null, tax_line_in: null,
};

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return {
    ...real,
    commands: {
      accountNumberMasked: () => Promise.resolve(""),
      accountDefaults: vi.fn((name: string, t: string) => ok({ ...brokerage, name, account_type: t, investment: t === "checking" ? null : brokerage.investment })),
    },
  };
});

import AccountModal from "./AccountModal.svelte";
import { dialogState } from "../state/dialogs.svelte";
import { listsState } from "../state/lists.svelte";

const checking = {
  ...brokerage, id: 7, name: "Checking", account_type: "checking", group: "banking", investment: null,
  status: "open", closed_date: null, created_at: "2026-01-01T00:00:00Z",
} as never;

beforeEach(() => {
  cleanup();
  listsState.today = "2026-06-30";
  listsState.accounts = [];
  dialogState.history = null;
});

describe("AccountModal", () => {
  it("has no icon bar checkbox (ACCT-100)", () => {
    render(AccountModal, { account: checking });
    expect(screen.getByLabelText(/Show in account list/)).toBeTruthy();
    expect(screen.queryByLabelText(/icon bar/)).toBeNull();
  });

  it("History… shows the account's audit history (AUD-020)", async () => {
    render(AccountModal, { account: checking });
    await fireEvent.click(screen.getByRole("button", { name: "History…" }));
    expect(dialogState.history).toEqual({ entity: "account", id: 7 });
  });

  it("a new investment account starts with the book's lot method (SET-040)", async () => {
    render(AccountModal, { account: null });
    const type = await screen.findByLabelText(/^Type/);
    await fireEvent.change(type, { target: { value: "brokerage" } });
    await waitFor(() => expect((screen.getByLabelText(/Default lot method/) as HTMLSelectElement).value).toBe("hifo"));
  });
});
