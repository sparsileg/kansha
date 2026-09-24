import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";

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
import { registerState } from "../state/register.svelte";
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
  listsState.accounts = [
    { id: 1, name: "Savings", account_type: "savings", group: "banking", status: "open", investment: null },
    { id: 2, name: "Checking", account_type: "checking", group: "banking", status: "open", investment: null },
  ] as never;
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

/** Type into a Category box and take the top match with Enter. */
async function pick(label: string, query: string) {
  const box = screen.getByLabelText(label);
  await fireEvent.input(box, { target: { value: query } });
  await fireEvent.keyDown(box, { key: "Enter" });
}

const listed = () =>
  within(screen.getByRole("listbox"))
    .getAllByRole("option")
    .map((o) => o.textContent?.trim());

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
    await pick("Category", "fuel");
    await fireEvent.submit(field("Date").closest("form")!);
    await waitFor(() => expect(c.entryCreate).toHaveBeenCalledTimes(1));
    const [entry, name] = c.entryCreate.mock.calls[0];
    expect(name).toBe("Shell");
    expect(entry.amount).toBe("-12.50");
    expect(entry.date).toBe("2026-09-24");
    expect(entry.lines[0].target).toEqual({ kind: "category", id: 6 });
    await waitFor(() => expect(field("Payment").value).toBe(""));
  });

  it("saves after picking a category and resets for the next entry", async () => {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payment"), { target: { value: "9" } });
    await pick("Category", "food");
    const cat = screen.getByLabelText("Category");
    expect((cat as HTMLInputElement).value).toBe("Food");
    await fireEvent.submit(cat.closest("form")!);
    await waitFor(() => expect(c.entryCreate).toHaveBeenCalledTimes(1));
    await screen.findByText(/Saved 09\/24\/2026/);
    expect(field("Payment").value).toBe("");
    expect(document.activeElement).toBe(field("Date"));
  });

  it("shows an error when the command rejects with a bare string", async () => {
    c.entryCreate.mockImplementation(() =>
      Promise.resolve({ status: "error" as const, error: "invalid args `entry`" as never }),
    );
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payment"), { target: { value: "9" } });
    await pick("Category", "food");
    await fireEvent.submit(field("Date").closest("form")!);
    expect((await screen.findByRole("alert")).textContent).toContain("invalid args");
  });

  it("typing a deposit clears the payment", async () => {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payment"), { target: { value: "5" } });
    await fireEvent.input(field("Deposit"), { target: { value: "7" } });
    expect(field("Payment").value).toBe("");
  });

  it("Enter on an empty new row does nothing, not even an error", async () => {
    render(EntryEditor, { account: 1 });
    await fireEvent.submit(field("Date").closest("form")!);
    expect(screen.queryByRole("alert")).toBeNull();
    expect(c.entryCreate).not.toHaveBeenCalled();
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
    expect((screen.getByLabelText("Category") as HTMLInputElement).value).toBe("Food");
  });
});

describe("Payee completion (PAY-020)", () => {
  it("Tab on a unique memorized match takes it, fills defaults, and moves on", async () => {
    render(EntryEditor, { account: 1 });
    const payee = field("Payee");
    await fireEvent.input(payee, { target: { value: "cost" } });
    await fireEvent.keyDown(payee, { key: "Tab" });
    expect(payee.value).toBe("Costco");
    expect(field("Payment").value).toBe("40.00");
    expect((screen.getByLabelText("Category") as HTMLInputElement).value).toBe("Food");
  });

  it("Enter takes it and moves to Payment without saving", async () => {
    render(EntryEditor, { account: 1 });
    const payee = field("Payee");
    await fireEvent.input(payee, { target: { value: "cos" } });
    await fireEvent.keyDown(payee, { key: "Enter" });
    expect(payee.value).toBe("Costco");
    expect(document.activeElement).toBe(field("Payment"));
    expect(c.entryCreate).not.toHaveBeenCalled();
  });

  it("does nothing when nothing, or more than one payee, matches", async () => {
    listsState.payees = [
      ...listsState.payees,
      { id: 2, name: "Costa Rica Trip", default_category: null, default_tag: null, default_memo: "", default_amount: null, hidden: false, created_at: "" },
    ];
    render(EntryEditor, { account: 1 });
    const payee = field("Payee");
    await fireEvent.input(payee, { target: { value: "cost" } });
    await fireEvent.keyDown(payee, { key: "Tab" });
    expect(payee.value).toBe("cost");
    await fireEvent.input(payee, { target: { value: "zzz" } });
    await fireEvent.keyDown(payee, { key: "Tab" });
    expect(payee.value).toBe("zzz");
  });
});

describe("editing an existing transaction", () => {
  const entry = {
    account: 1, date: "2026-09-23", payee: 1, check_num: "", memo: "m", notes: "",
    amount: "-3500.00", cleared: "unmarked" as const, tags: [],
    lines: [{ target: { kind: "account" as const, id: 2 }, amount: "-3500.00", memo: "", cleared: "unmarked" as const, tags: [] }],
  };

  it("Save calls entryUpdate and then closes the editor", async () => {
    c.entryGet.mockImplementation(() => ok(entry));
    c.entryUpdate.mockImplementation(() => ok(null));
    const ondone = vi.fn();
    render(EntryEditor, { account: 1, txn: 7, ondone });
    await waitFor(() => expect(field("Payment").value).toBe("3,500.00"));
    await fireEvent.input(field("Memo"), { target: { value: "changed" } });
    await fireEvent.submit(field("Date").closest("form")!);
    await waitFor(() => expect(c.entryUpdate).toHaveBeenCalledTimes(1));
    const [id, e, name, confirmed] = c.entryUpdate.mock.calls[0];
    expect([id, name, confirmed]).toEqual([7, "Costco", false]);
    expect(e.memo).toBe("changed");
    await waitFor(() => expect(ondone).toHaveBeenCalled());
  });

  it("Enter on an untouched edit writes nothing and closes as saved", async () => {
    c.entryGet.mockImplementation(() => ok(entry));
    const ondone = vi.fn();
    render(EntryEditor, { account: 1, txn: 7, ondone });
    await waitFor(() => expect(field("Payment").value).toBe("3,500.00"));
    await fireEvent.submit(field("Date").closest("form")!);
    expect(c.entryUpdate).not.toHaveBeenCalled();
    expect(ondone).toHaveBeenCalledWith(true);
  });

  it("asks to confirm when Rust requires it, then repeats", async () => {
    c.entryGet.mockImplementation(() => ok(entry));
    c.entryUpdate
      .mockImplementationOnce(() => Promise.resolve({ status: "error" as const, error: { kind: "confirmation_required" as const, message: "Reconciled. Change it?" } }))
      .mockImplementation(() => ok(null));
    const { confirmState } = await import("../state/confirm.svelte");
    const ask = vi.spyOn(confirmState, "ask").mockResolvedValue(true);
    const ondone = vi.fn();
    render(EntryEditor, { account: 1, txn: 7, ondone });
    await waitFor(() => expect(field("Payment").value).toBe("3,500.00"));
    await fireEvent.input(field("Memo"), { target: { value: "changed" } });
    await fireEvent.submit(field("Date").closest("form")!);
    await waitFor(() => expect(c.entryUpdate).toHaveBeenCalledTimes(2));
    expect(ask).toHaveBeenCalledWith("Reconciled. Change it?");
    expect(c.entryUpdate.mock.calls[1][3]).toBe(true);
    await waitFor(() => expect(ondone).toHaveBeenCalled());
  });
});

describe("Category type-ahead (REG-030)", () => {
  it("finds an account by typing its name without brackets", async () => {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payment"), { target: { value: "50" } });
    const box = screen.getByLabelText("Category");
    await fireEvent.input(box, { target: { value: "check" } });
    expect(listed()).toContain("[Checking]");
    await fireEvent.keyDown(box, { key: "Enter" });
    expect((box as HTMLInputElement).value).toBe("[Checking]");
    await fireEvent.submit(box.closest("form")!);
    await waitFor(() => expect(c.entryCreate).toHaveBeenCalledTimes(1));
    expect(c.entryCreate.mock.calls[0][0].lines[0].target).toEqual({ kind: "account", id: 2 });
  });

  it("lists categories and accounts together and never the account being edited", async () => {
    render(EntryEditor, { account: 1 });
    const box = screen.getByLabelText("Category");
    await fireEvent.input(box, { target: { value: "f" } });
    expect(listed()).toEqual(expect.arrayContaining(["Food", "Fuel"]));
    await fireEvent.input(box, { target: { value: "savings" } });
    expect(listed()).toEqual(["No match"]);
  });

  it("Tab takes the highlighted match; Esc closes the list without cancelling the entry", async () => {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payment"), { target: { value: "5" } });
    const box = screen.getByLabelText("Category") as HTMLInputElement;
    await fireEvent.input(box, { target: { value: "fu" } });
    await fireEvent.keyDown(box, { key: "Escape" });
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(field("Payment").value).toBe("5");
    await fireEvent.input(box, { target: { value: "fu" } });
    await fireEvent.keyDown(box, { key: "Tab" });
    expect(box.value).toBe("Fuel");
  });
});

describe("amount fields accept only numeric characters", () => {
  it("strips letters, signs, and extra decimals as they are entered", async () => {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payment"), { target: { value: "12a.5x" } });
    expect(field("Payment").value).toBe("12.5");
    await fireEvent.input(field("Deposit"), { target: { value: "-1,000.999" } });
    expect(field("Deposit").value).toBe("1,000.99");
    expect(field("Payment").value).toBe("");
  });

  it("blocks a non-numeric key before it is inserted", () => {
    render(EntryEditor, { account: 1 });
    const ev = new InputEvent("beforeinput", { data: "a", cancelable: true, bubbles: true });
    field("Payment").dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(true);
    const ok = new InputEvent("beforeinput", { data: "5", cancelable: true, bubbles: true });
    field("Payment").dispatchEvent(ok);
    expect(ok.defaultPrevented).toBe(false);
  });
});

describe("split amounts offer what is left", () => {
  // A stand-in for Rust: total minus parts, in cents (test-only arithmetic).
  const cents = (m: string) => Math.round(parseFloat(m) * 100);
  const fmt = (n: number) => `${n < 0 ? "-" : ""}${Math.floor(Math.abs(n) / 100)}.${String(Math.abs(n) % 100).padStart(2, "0")}`;
  beforeEach(() => {
    c.splitRemainder.mockImplementation((total: string, parts: string[]) =>
      ok(fmt(cents(total) - parts.reduce((a, p) => a + cents(p), 0))),
    );
  });

  it("entering Split mode puts the whole amount on line 1 and focuses it", async () => {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payment"), { target: { value: "100" } });
    await pick("Category", "split");
    await waitFor(() => expect(field("Split 1 amount").value).toBe("100.00"));
    expect(field("Split 2 amount").value).toBe("");
    expect(document.activeElement).toBe(screen.getByLabelText("Split 1 category"));
  });

  it("moving to the next line offers the updated balance", async () => {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payment"), { target: { value: "100" } });
    await pick("Category", "split");
    await waitFor(() => expect(field("Split 1 amount").value).toBe("100.00"));
    await fireEvent.input(field("Split 1 amount"), { target: { value: "60" } });
    await fireEvent.focusIn(screen.getByLabelText("Split line 2"));
    await waitFor(() => expect(field("Split 2 amount").value).toBe("40.00"));
  });

  it("offers nothing once the split is complete", async () => {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payment"), { target: { value: "100" } });
    await pick("Category", "split");
    await waitFor(() => expect(field("Split 1 amount").value).toBe("100.00"));
    await fireEvent.focusIn(screen.getByLabelText("Split line 2"));
    await new Promise((r) => setTimeout(r, 20));
    expect(field("Split 2 amount").value).toBe("");
  });

  it("Tab out of the last line with an amount left opens a new line", async () => {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payment"), { target: { value: "100" } });
    await pick("Category", "split");
    await waitFor(() => expect(field("Split 1 amount").value).toBe("100.00"));
    await fireEvent.input(field("Split 1 amount"), { target: { value: "60" } });
    await fireEvent.input(field("Split 2 amount"), { target: { value: "10" } });
    await screen.findByText(/Remainder: -?30\.00/);
    await fireEvent.keyDown(field("Split 2 memo"), { key: "Tab" });
    await waitFor(() => expect(field("Split 3 amount").value).toBe("30.00"));
  });
});

describe("split panel placement", () => {
  async function open() {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payment"), { target: { value: "100" } });
    await pick("Category", "split");
    const panel = await screen.findByRole("group", { name: "Split lines" });
    const cells = field("Date").parentElement!;
    return { panel, cells };
  }
  const before = (a: Element, b: Element) =>
    !!(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);

  it("is above the entry line when the register sorts ascending", async () => {
    registerState.descending = false;
    const { panel, cells } = await open();
    expect(before(panel, cells)).toBe(true);
  });

  it("is below the entry line when the register sorts descending", async () => {
    registerState.descending = true;
    const { panel, cells } = await open();
    expect(before(cells, panel)).toBe(true);
    registerState.descending = false;
  });
});

describe("saving does not change the entry row's height", () => {
  it("keeps one message line whether or not there is a message", async () => {
    const { container } = render(EntryEditor, { account: 1 });
    expect(container.querySelector(".msg")).toBeTruthy();
    await fireEvent.input(field("Payment"), { target: { value: "9" } });
    await pick("Category", "food");
    await fireEvent.submit(field("Date").closest("form")!);
    await screen.findByText(/Saved 09\/24\/2026/);
    expect(container.querySelectorAll(".msg")).toHaveLength(1);
  });
});

describe("split remainder validation (TXN-020)", () => {
  async function toSplit() {
    render(EntryEditor, { account: 1 });
    await fireEvent.input(field("Payment"), { target: { value: "100" } });
    await pick("Category", "split");
    await pick("Split 1 category", "food");
    await pick("Split 2 category", "fuel");
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
