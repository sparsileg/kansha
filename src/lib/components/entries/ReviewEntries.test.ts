import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/svelte";
import type { EntryEdit, InboxReport, PendingList, PendingView } from "../../types/bindings";

const ok = (n: string) => ({ given: n.toLowerCase(), resolved: n, error: null });

const plain: PendingView = {
  id: 1,
  sender: "Pat's phone",
  source: "file",
  file_name: "a.txt",
  received_at: "2026-10-12T10:00:00Z",
  date: "2026-10-12",
  payee: "Kroger",
  amount: "45.20",
  account: ok("Checking"),
  category: { given: "Grocries", resolved: null, error: "no category “Grocries”; did you mean Food:Groceries?" },
  splits: [],
  memo: "week",
  ready: false,
};

const split: PendingView = {
  ...plain,
  id: 2,
  payee: "Target",
  amount: "107.00",
  category: null,
  splits: [
    { amount: "7.00", category: ok("Taxes:Sales"), memo: "" },
    { amount: "100.00", category: ok("Household"), memo: "" },
  ],
  ready: true,
};

let list: PendingList = { entries: [], accounts: [], categories: [] };
let inbox: InboxReport = { received: [], refused: [] };
let picked: string | null = null;
let inboxReads = 0;
const approved: number[][] = [];
const rejected: number[][] = [];
const edits: { id: number; edit: EntryEdit }[] = [];

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      entriesPending: () => Promise.resolve({ status: "ok", data: list }),
      entriesCount: () => Promise.resolve({ status: "ok", data: list.entries.length }),
      entriesApprove: (ids: number[]) => {
        approved.push(ids);
        list = { ...list, entries: list.entries.filter((e) => !ids.includes(e.id)) };
        return Promise.resolve({ status: "ok", data: { txns: ids.map((i) => 100 + i), account: 7 } });
      },
      entriesReject: (ids: number[]) => {
        rejected.push(ids);
        list = { ...list, entries: list.entries.filter((e) => !ids.includes(e.id)) };
        return Promise.resolve({ status: "ok", data: null });
      },
      entriesEdit: (id: number, edit: EntryEdit) => {
        edits.push({ id, edit });
        return Promise.resolve({ status: "ok", data: plain });
      },
      entriesInboxRead: () => {
        inboxReads++;
        return Promise.resolve({ status: "ok", data: inbox });
      },
      entriesImport: () => Promise.resolve({ status: "ok", data: inbox }),
      pickEntriesFile: () => Promise.resolve(picked),
      accountBalances: () => Promise.resolve({ status: "ok", data: [] }),
      sectionTotals: () => Promise.resolve({ status: "ok", data: [] }),
    },
  };
});

vi.mock("../../shell/nav", () => ({ openAccount: vi.fn(() => Promise.resolve()) }));

import ReviewEntries from "./ReviewEntries.svelte";
import { confirmState } from "../../state/confirm.svelte";
import { dialogState } from "../../state/dialogs.svelte";
import { entriesState, reviewLabel } from "../../state/entries.svelte";
import { registerState } from "../../state/register.svelte";
import { statusState } from "../../state/status.svelte";
import { openAccount } from "../../shell/nav";

beforeEach(() => {
  cleanup();
  list = { entries: [plain, split], accounts: ["Checking"], categories: ["Food:Groceries", "Household"] };
  approved.length = 0;
  rejected.length = 0;
  edits.length = 0;
  dialogState.reviewEntries = true;
  statusState.clear();
});

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("ReviewEntries", () => {
  it("a read asked for while one runs runs after it, once", async () => {
    inboxReads = 0;
    const first = entriesState.readInbox();
    void entriesState.readInbox();
    void entriesState.readInbox();
    await first;
    await tick();
    await tick();
    expect(inboxReads).toBe(2);
  });

  it("marks unresolved names with ⚠ and the reason, not colour alone", async () => {
    render(ReviewEntries);
    await tick();
    expect(screen.getByText("⚠ Grocries")).toBeTruthy();
    expect(screen.getByText(/did you mean Food:Groceries/)).toBeTruthy();
    expect(screen.getByText("Split (2)")).toBeTruthy();
  });

  it("offers Approve only when ready, and Approve and open for a split", async () => {
    render(ReviewEntries);
    await tick();
    const approve = screen.getByRole("button", { name: "Approve" }) as HTMLButtonElement;
    expect(approve.disabled).toBe(true);
    expect(approve.title).toMatch(/Fix the names/);
    const all = screen.getByRole("button", { name: "Approve all ready (1)" });
    await fireEvent.click(all);
    await tick();
    expect(approved).toEqual([[2]]);
    expect(statusState.message?.text).toMatch(/Edit > Undo takes it back/);
  });

  it("Approve and open approves the split and opens it in the register", async () => {
    const go = vi.spyOn(registerState, "goToTransaction").mockResolvedValue();
    render(ReviewEntries);
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Approve and open" }));
    await tick();
    expect(approved).toEqual([[2]]);
    expect(openAccount).toHaveBeenCalledWith(7);
    expect(go).toHaveBeenCalledWith(7, 102);
    expect(registerState.editing).toBe(102);
    expect(registerState.editSplit).toBe(true);
    expect(dialogState.reviewEntries).toBe(false);
    // The edit ends with entries still waiting: the list comes back.
    await entriesState.editEnded(102);
    expect(dialogState.reviewEntries).toBe(true);
    registerState.editing = null;
    registerState.editSplit = false;
  });

  it("does not come back when nothing waits, or after leaving the register", async () => {
    vi.spyOn(registerState, "goToTransaction").mockResolvedValue();
    list = { ...list, entries: [split] };
    render(ReviewEntries);
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Approve and open" }));
    await tick();
    await entriesState.editEnded(102);
    expect(dialogState.reviewEntries).toBe(false);

    list = { ...list, entries: [plain] };
    entriesState.reopenAfter = 102;
    registerState.close();
    await entriesState.editEnded(102);
    expect(dialogState.reviewEntries).toBe(false);
    registerState.editing = null;
    registerState.editSplit = false;
  });

  it("edits a plain entry", async () => {
    render(ReviewEntries);
    await tick();
    await fireEvent.click(screen.getAllByRole("button", { name: "Edit" })[0]);
    const cat = screen.getByLabelText("Category") as HTMLInputElement;
    await fireEvent.input(cat, { target: { value: "Food:Groceries" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await tick();
    expect(edits).toHaveLength(1);
    expect(edits[0].edit).toMatchObject({
      date: "2026-10-12",
      payee: "Kroger",
      amount: "45.20",
      account: "Checking",
      category: "Food:Groceries",
      memo: "week",
    });
  });

  it("refreshes an open register after approving", async () => {
    const refresh = vi.spyOn(registerState, "refresh").mockResolvedValue();
    render(ReviewEntries);
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Approve all ready (1)" }));
    await tick();
    expect(refresh).toHaveBeenCalled();
    refresh.mockRestore();
  });

  it("focuses the first ready entry's approve button, not the close button", async () => {
    render(ReviewEntries);
    await tick();
    await tick();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Approve and open" }));
  });

  it("focuses the next ready entry after an approve", async () => {
    const third: PendingView = { ...plain, id: 3, payee: "Shell", category: ok("Auto:Fuel"), ready: true };
    list = { ...list, entries: [plain, third, { ...third, id: 4, payee: "BP" }] };
    vi.spyOn(registerState, "refresh").mockResolvedValue();
    render(ReviewEntries);
    await tick();
    await tick();
    const first = screen.getAllByRole("button", { name: "Approve" });
    expect(document.activeElement).toBe(first[1]);
    await fireEvent.click(first[1]);
    await tick();
    await tick();
    expect(approved).toEqual([[3]]);
    const next = screen.getAllByRole("button", { name: "Approve" });
    expect(next).toHaveLength(2);
    expect(document.activeElement).toBe(next[1]);
  });

  it("focuses Edit on the first entry when none is ready", async () => {
    list = { ...list, entries: [plain] };
    render(ReviewEntries);
    await tick();
    await tick();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Edit" }));
  });

  it("completes category and account names while editing", async () => {
    render(ReviewEntries);
    await tick();
    await fireEvent.click(screen.getAllByRole("button", { name: "Edit" })[0]);
    const cat = screen.getByLabelText("Category") as HTMLInputElement;
    await fireEvent.input(cat, { target: { value: "groc" } });
    expect(within(screen.getByRole("listbox")).getAllByRole("option").map((o) => o.textContent?.trim()))
      .toEqual(["Food:Groceries"]);
    await fireEvent.keyDown(cat, { key: "Enter" });
    expect(cat.value).toBe("Food:Groceries");
    // A path level fills in as in the register: "H" → "Household".
    await fireEvent.input(cat, { target: { value: "H" } });
    expect(cat.value).toBe("Household");
    const acct = screen.getByLabelText("Account") as HTMLInputElement;
    await fireEvent.input(acct, { target: { value: "che" } });
    await fireEvent.keyDown(acct, { key: "Tab" });
    expect(acct.value).toBe("Checking");
    // A name not in the list stays as typed; review checks it.
    await fireEvent.input(cat, { target: { value: "Zzz" } });
    await fireEvent.blur(cat);
    expect(cat.value).toBe("Zzz");
  });

  it("Later becomes Close, focused, once nothing waits", async () => {
    list = { ...list, entries: [split] };
    vi.spyOn(registerState, "refresh").mockResolvedValue();
    vi.spyOn(registerState, "goToTransaction").mockResolvedValue();
    render(ReviewEntries);
    await tick();
    expect(screen.getByRole("button", { name: "Later" })).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Approve all ready (1)" }));
    await tick();
    await tick();
    expect(screen.queryByRole("button", { name: "Later" })).toBeNull();
    expect(screen.queryByRole("button", { name: /Approve all ready/ })).toBeNull();
    // The footer's Close, not the title bar's ×.
    const close = screen.getAllByRole("button", { name: "Close" }).find((b) => b.closest("footer"))!;
    expect(document.activeElement).toBe(close);
    await fireEvent.click(close);
    expect(dialogState.reviewEntries).toBe(false);
  });

  it("asks before rejecting", async () => {
    const ask = vi.spyOn(confirmState, "ask").mockResolvedValue(true);
    render(ReviewEntries);
    await tick();
    await fireEvent.click(screen.getAllByRole("button", { name: "Reject" })[0]);
    await tick();
    expect(ask).toHaveBeenCalled();
    expect(rejected).toEqual([[1]]);
  });
});

describe("entriesState", () => {
  it("opens the list at startup only when entries wait", async () => {
    dialogState.reviewEntries = false;
    list = { ...list, entries: [] };
    await entriesState.startup();
    expect(dialogState.reviewEntries).toBe(false);
    list = { ...list, entries: [plain] };
    await entriesState.startup();
    expect(dialogState.reviewEntries).toBe(true);
  });

  it("entries arriving later post a note and never open the list", async () => {
    dialogState.reviewEntries = false;
    await entriesState.arrived(2);
    expect(dialogState.reviewEntries).toBe(false);
    expect(entriesState.count).toBe(2);
    expect(statusState.message?.text).toBe("2 new entries to review (File > Review Entries…).");
  });

  it("an inbox file that fails is reported once, not on every focus", async () => {
    inbox = {
      received: [],
      refused: [{ file_name: "x.txt", errors: [{ line: 2, message: "“ten” is not an amount" }] }],
    };
    await entriesState.readInbox();
    expect(statusState.message).toEqual({ text: "x.txt stays in the inbox: line 2: “ten” is not an amount.", kind: "alert" });
    statusState.clear();
    await entriesState.readInbox();
    expect(statusState.message).toBeNull();
    inbox = { received: [], refused: [] };
  });

  it("entries read on focus post a note", async () => {
    dialogState.reviewEntries = false;
    inbox = { received: [{ file_name: "a.txt", added: 1, skipped: 0 }], refused: [] };
    await entriesState.readInbox();
    expect(statusState.message?.text).toBe("1 new entry to review (File > Review Entries…).");
    expect(dialogState.reviewEntries).toBe(false);
    inbox = { received: [], refused: [] };
  });

  it("Import Entries… opens the list after importing", async () => {
    dialogState.reviewEntries = false;
    picked = "/home/stan/Downloads/from-signal.txt";
    inbox = { received: [{ file_name: "from-signal.txt", added: 2, skipped: 1 }], refused: [] };
    await entriesState.importFile();
    expect(statusState.message?.text).toBe("Imported 2 entries for review; 1 seen before were skipped.");
    expect(dialogState.reviewEntries).toBe(true);
    picked = null;
    inbox = { received: [], refused: [] };
  });

  it("the menu label carries the count", () => {
    expect(reviewLabel(0)).toBe("Review Entries…");
    expect(reviewLabel(3)).toBe("Review Entries… (3)");
  });
});
