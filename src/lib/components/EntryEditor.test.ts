import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return {
    ...real,
    commands: {
      entryCreate: vi.fn(),
      entryUpdate: vi.fn(),
      entryGet: vi.fn(),
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
import EntryEditor from "./EntryEditor.svelte";

const c = vi.mocked(commands, true);
const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });

beforeEach(() => {
  cleanup();
  vi.clearAllMocks();
  listsState.today = "2026-09-24";
  listsState.categories = [
    { id: 5, parent: null, kind: "expense", name: "Food", system: null, tax_related: false, tithable: false, giving: false, hidden: false, created_at: "" },
    { id: 6, parent: null, kind: "expense", name: "Fuel", system: null, tax_related: false, tithable: false, giving: false, hidden: false, created_at: "" },
  ];
  listsState.payees = [
    { id: 1, name: "Costco", default_category: 5, default_tag: null, default_memo: "bulk", default_amount: "-40.00", hidden: false, created_at: "" },
  ];
  c.entryCreate.mockImplementation(() => ok(1));
  c.payeeSearch.mockImplementation(() => ok([]));
  c.payeeList.mockImplementation(() => ok([]));
  c.registerQuery.mockImplementation(() => ok({ rows: [], total: 0, today: "2026-09-24" }));
  c.registerSummary.mockImplementation(() => ok({ current: "0.00", cleared: "0.00", ending: "0.00", available_credit: null }));
  c.accountBalances.mockImplementation(() => ok([]));
  c.splitRemainder.mockImplementation(() => ok("0.00"));
});

const field = (name: string) => screen.getByLabelText(name) as HTMLInputElement;

describe("EntryEditor keyboard entry (REG-030)", () => {
  it("+ / - / t adjust the date field", async () => {
    render(EntryEditor, { account: 1 });
    const date = field("Date");
    expect(date.value).toBe("09/24/2026");
    await fireEvent.keyDown(date, { key: "+" });
    expect(date.value).toBe("09/25/2026");
    await fireEvent.keyDown(date, { key: "-" });
    await fireEvent.keyDown(date, { key: "-" });
    expect(date.value).toBe("09/23/2026");
    await fireEvent.keyDown(date, { key: "t" });
    expect(date.value).toBe("09/24/2026");
  });

  it("Enter saves a payment and resets for the next entry", async () => {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payee"), { target: { value: "Shell" } });
    await fireEvent.input(field("Payment"), { target: { value: "12.5" } });
    await fireEvent.change(screen.getByLabelText("Category"), { target: { value: "c:6" } });
    await fireEvent.submit(field("Date").closest("form")!);
    await waitFor(() => expect(c.entryCreate).toHaveBeenCalledTimes(1));
    const [entry, name] = c.entryCreate.mock.calls[0];
    expect(name).toBe("Shell");
    expect(entry.amount).toBe("-12.50");
    expect(entry.date).toBe("2026-09-24");
    expect(entry.lines[0].target).toEqual({ kind: "category", id: 6 });
    await waitFor(() => expect(field("Payment").value).toBe(""));
  });

  it("typing a deposit clears the payment", async () => {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payment"), { target: { value: "5" } });
    await fireEvent.input(field("Deposit"), { target: { value: "7" } });
    expect(field("Payment").value).toBe("");
  });

  it("shows a message and does not save without a category", async () => {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payment"), { target: { value: "5" } });
    await fireEvent.submit(field("Date").closest("form")!);
    expect((await screen.findByRole("alert")).textContent).toContain("category");
    expect(c.entryCreate).not.toHaveBeenCalled();
  });

  it("Escape cancels and clears the row", async () => {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payment"), { target: { value: "5" } });
    await fireEvent.keyDown(field("Payment"), { key: "Escape" });
    expect(field("Payment").value).toBe("");
  });

  it("QuickFill fills empty fields from the payee's defaults (PAY-020)", async () => {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payee"), { target: { value: "costco" } });
    await fireEvent.change(field("Payee"));
    await waitFor(() => expect(field("Payment").value).toBe("40.00"));
    expect(field("Memo").value).toBe("bulk");
    expect((screen.getByLabelText("Category") as HTMLSelectElement).value).toBe("c:5");
  });
});

describe("split remainder validation (TXN-020)", () => {
  async function toSplit() {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payment"), { target: { value: "100" } });
    await fireEvent.change(screen.getByLabelText("Category"), { target: { value: "split" } });
    await fireEvent.change(screen.getByLabelText("Split 1 category"), { target: { value: "c:5" } });
    await fireEvent.change(screen.getByLabelText("Split 2 category"), { target: { value: "c:6" } });
    await fireEvent.input(field("Split 1 amount"), { target: { value: "60" } });
    await fireEvent.input(field("Split 2 amount"), { target: { value: "30" } });
  }

  it("asks Rust for the remainder with signed amounts and blocks a non-zero one", async () => {
    c.splitRemainder.mockImplementation(() => ok("-10.00"));
    await toSplit();
    await waitFor(() =>
      expect(c.splitRemainder).toHaveBeenLastCalledWith("-100.00", ["-60.00", "-30.00"]),
    );
    await screen.findByText(/Remainder: -10\.00/);
    await fireEvent.submit(field("Date").closest("form")!);
    expect((await screen.findByRole("alert")).textContent).toContain("add up");
    expect(c.entryCreate).not.toHaveBeenCalled();
  });

  it("saves when the remainder is zero", async () => {
    c.splitRemainder.mockImplementation(() => ok("0.00"));
    await toSplit();
    await screen.findByText(/Remainder: 0\.00/);
    await fireEvent.submit(field("Date").closest("form")!);
    await waitFor(() => expect(c.entryCreate).toHaveBeenCalledTimes(1));
    expect(c.entryCreate.mock.calls[0][0].lines).toHaveLength(2);
  });
});
