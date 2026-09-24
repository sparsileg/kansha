import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return {
    ...real,
    commands: {
      entryCreate: vi.fn(),
      scheduleEnter: vi.fn(),
      scheduleList: vi.fn(),
      scheduleDueList: vi.fn(),
      scheduleReviewList: vi.fn(),
      payeeSearch: vi.fn(),
      payeeList: vi.fn(),
      splitRemainder: vi.fn(),
      registerQuery: vi.fn(),
      registerSummary: vi.fn(),
      accountBalances: vi.fn(),
    },
  };
});

import { commands } from "../api";
import { listsState } from "../state/lists.svelte";
import { registerState } from "../state/register.svelte";
import EntryEditor from "./EntryEditor.svelte";

const c = vi.mocked(commands, true);
const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });

beforeEach(() => {
  cleanup();
  vi.clearAllMocks();
  listsState.today = "2026-09-24";
  listsState.categories = [
    { id: 5, parent: null, kind: "expense", name: "Rent", system: null, tax_related: false, tithable: false, giving: false, hidden: false, created_at: "" },
  ];
  listsState.accounts = [
    { id: 2, name: "Checking", account_type: "checking", group: "banking", status: "open", investment: null },
  ] as never;
  listsState.payees = [
    { id: 1, name: "Landlord", default_category: null, default_tag: null, default_memo: "", default_amount: null, hidden: false, created_at: "" },
  ];
  c.scheduleEnter.mockImplementation(() => ok({ schedule: 1, nominal: "2026-10-01", txn: 9, date: "2026-10-01" }));
  c.scheduleList.mockImplementation(() => ok([]));
  c.scheduleDueList.mockImplementation(() => ok([]));
  c.scheduleReviewList.mockImplementation(() => ok([]));
  c.payeeSearch.mockImplementation(() => ok([]));
  c.payeeList.mockImplementation(() => ok([]));
  c.registerQuery.mockImplementation(() => ok({ rows: [], total: 0, today: "2026-09-24" }));
  c.registerSummary.mockImplementation(() => ok({ current: "0.00", cleared: "0.00", ending: "0.00", available_credit: null }));
  c.accountBalances.mockImplementation(() => ok([]));
  registerState.accountId = 2;
});

describe("EntryEditor with a scheduled occurrence", () => {
  it("takes the prefill, focuses the amount, and saves through scheduleEnter", async () => {
    render(EntryEditor, { account: 2 });
    registerState.prefill = {
      schedule: 1,
      due: "2026-10-01",
      entry: {
        account: 2,
        date: "2026-10-01",
        payee: 1,
        check_num: "",
        memo: "rent",
        notes: "",
        amount: "-1000.00",
        cleared: "unmarked",
        tags: [],
        lines: [{ target: { kind: "category", id: 5 }, amount: "-1000.00", memo: "", cleared: "unmarked", tags: [] }],
      },
    };
    const pay = (await screen.findByLabelText("Payment")) as HTMLInputElement;
    await waitFor(() => expect(pay.value).toBe("1,000.00"));
    await waitFor(() => expect(document.activeElement).toBe(pay));
    expect((screen.getByLabelText("Payee") as HTMLInputElement).value).toBe("Landlord");
    expect(registerState.prefill).toBeNull();

    // The usual edit: change the amount, then Enter.
    await fireEvent.input(pay, { target: { value: "1,050.00" } });
    await fireEvent.click(screen.getByRole("button", { name: "Enter" }));
    await waitFor(() => expect(c.scheduleEnter).toHaveBeenCalled());
    const [schedule, due, edits, payeeName] = c.scheduleEnter.mock.calls[0];
    expect([schedule, due, payeeName]).toEqual([1, "2026-10-01", "Landlord"]);
    expect(edits.entry?.amount).toBe("-1050.00");
    expect(c.entryCreate).not.toHaveBeenCalled();
  });
});
