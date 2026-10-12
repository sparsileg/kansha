import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import type { FindQuery, FoundKey, FoundRow, ReplaceSpec } from "../types/bindings";

const found: FoundRow[] = [
  {
    key: { txn: 3, account: 1, line: null },
    date: "2026-04-10",
    account_name: "Checking",
    check_num: "",
    payee: "Netflix.com",
    category: "Entertainment",
    tags: "",
    memo: "",
    cleared: "cleared",
    amount: "-17.99",
  },
  {
    key: { txn: 2, account: 4, line: null },
    date: "2026-03-10",
    account_name: "Visa",
    check_num: "",
    payee: "Netflix.com",
    category: "Entertainment",
    tags: "Business",
    memo: "",
    cleared: "unmarked",
    amount: "-16.46",
  },
];

const finds: FindQuery[] = [];
const replaces: { rows: FoundKey[]; spec: ReplaceSpec; confirmed: boolean }[] = [];
let askFirst = false;

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return {
    ...real,
    commands: {
      findTransactions: (query: FindQuery) => {
        finds.push(query);
        return Promise.resolve({ status: "ok", data: { rows: found, txns: 2 } });
      },
      replaceTransactions: (rows: FoundKey[], spec: ReplaceSpec, confirmed: boolean) => {
        replaces.push({ rows, spec, confirmed });
        if (askFirst && !confirmed) {
          return Promise.resolve({
            status: "error",
            error: { kind: "confirmation_required", message: "1 of the checked transactions are reconciled. Replace anyway?" },
          });
        }
        return Promise.resolve({
          status: "ok",
          data: { changed: rows.length, unchanged: 0, skipped_split: 0, skipped_transfer: 0, skipped_closed: 0, skipped_gone: 0 },
        });
      },
    },
  };
});

import FindReplaceModal from "./FindReplaceModal.svelte";
import { confirmState } from "../state/confirm.svelte";
import { dialogState } from "../state/dialogs.svelte";
import { listsState } from "../state/lists.svelte";
import { scheduleState } from "../state/schedule.svelte";
import { runAction } from "../shell/actions";

beforeEach(() => {
  cleanup();
  finds.length = 0;
  replaces.length = 0;
  askFirst = false;
  listsState.today = "2026-10-11";
  vi.spyOn(listsState, "loadAll").mockResolvedValue();
  vi.spyOn(scheduleState, "changed").mockResolvedValue();
});

async function findNetflix() {
  render(FindReplaceModal, { onclose: () => {} });
  await fireEvent.input(screen.getByLabelText("Find value 1"), { target: { value: "netflix" } });
  await fireEvent.click(screen.getByRole("button", { name: "Find" }));
  await screen.findByText("Visa");
}

describe("File > Find/Replace (REG-170)", () => {
  it("the File menu item opens it", () => {
    dialogState.findReplace = false;
    runAction("file.find_replace");
    expect(dialogState.findReplace).toBe(true);
    dialogState.findReplace = false;
  });

  it("Find sends the row and checks every found row", async () => {
    await findNetflix();
    expect(finds[0]).toEqual({
      conditions: [{ field: "payee", comparison: "contains", value: "netflix" }],
      join: "or",
      within_splits: true,
      sort: "date",
      descending: true,
    });
    const boxes = screen.getAllByRole("checkbox", { name: /^Select / }) as HTMLInputElement[];
    expect(boxes).toHaveLength(2);
    expect(boxes.every((b) => b.checked)).toBe(true);
    expect(screen.getByText(/Found in 2 transactions/)).toBeTruthy();
    // Amounts as the register shows them; Clr as c / R.
    expect(screen.getByText("-17.99")).toBeTruthy();
    expect(screen.getByText("c")).toBeTruthy();
  });

  it("Add Row adds query rows with one Or/And for all of them", async () => {
    render(FindReplaceModal, { onclose: () => {} });
    expect(screen.queryByLabelText("Join rows")).toBeNull();
    await fireEvent.click(screen.getByText("Add Row"));
    await fireEvent.click(screen.getByText("Add Row"));
    expect(screen.getByLabelText("Find value 3")).toBeTruthy();
    expect(screen.getAllByLabelText("Join rows")).toHaveLength(1);
    await fireEvent.change(screen.getByLabelText("Join rows"), { target: { value: "and" } });
    await fireEvent.input(screen.getByLabelText("Find value 1"), { target: { value: "netflix" } });
    await fireEvent.click(screen.getByRole("button", { name: "Find" }));
    await screen.findByText("Visa");
    expect(finds[0].join).toBe("and");
    expect(finds[0].conditions).toHaveLength(3);
    await fireEvent.click(screen.getByLabelText("Remove row 3"));
    expect(screen.queryByLabelText("Find value 3")).toBeNull();
  });

  it("Replace All changes only the checked rows and says what it did", async () => {
    await findNetflix();
    const boxes = screen.getAllByRole("checkbox", { name: /^Select / }) as HTMLInputElement[];
    await fireEvent.click(boxes[0]);
    expect(screen.getByText(/1 of 2 rows checked/)).toBeTruthy();
    await fireEvent.input(screen.getByLabelText("Replace with"), { target: { value: "Subscription" } });
    await fireEvent.click(screen.getByText("Replace All"));
    expect(await screen.findByText("Changed 1 transaction.")).toBeTruthy();
    expect(replaces).toEqual([
      { rows: [{ txn: 2, account: 4, line: null }], spec: { field: "tag", with: "Subscription" }, confirmed: false },
    ]);
  });

  it("Clear All unchecks every row and Replace All is off", async () => {
    await findNetflix();
    await fireEvent.click(screen.getByText("Clear All"));
    const boxes = screen.getAllByRole("checkbox", { name: /^Select / }) as HTMLInputElement[];
    expect(boxes.some((b) => b.checked)).toBe(false);
    expect((screen.getByText("Replace All") as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(screen.getByText("Select All"));
    expect((screen.getByText("Replace All") as HTMLButtonElement).disabled).toBe(false);
  });

  it("asks once for the batch, then replaces confirmed", async () => {
    askFirst = true;
    const ask = vi.spyOn(confirmState, "ask").mockResolvedValue(true);
    await findNetflix();
    await fireEvent.input(screen.getByLabelText("Replace with"), { target: { value: "Streaming" } });
    await fireEvent.click(screen.getByText("Replace All"));
    expect(await screen.findByText("Changed 2 transactions.")).toBeTruthy();
    expect(ask).toHaveBeenCalledTimes(1);
    expect(replaces.map((r) => r.confirmed)).toEqual([false, true]);
  });

  it("a Date replacement is sent as ISO", async () => {
    vi.spyOn(confirmState, "ask").mockResolvedValue(true);
    await findNetflix();
    await fireEvent.change(screen.getByLabelText("Replace field"), { target: { value: "date" } });
    await fireEvent.input(screen.getByLabelText("Replace with"), { target: { value: "6/15/2026" } });
    await fireEvent.click(screen.getByText("Replace All"));
    await screen.findByText("Changed 2 transactions.");
    expect(replaces[0].spec).toEqual({ field: "date", with: "2026-06-15" });
  });

  it("sorting by a column finds again with that sort", async () => {
    await findNetflix();
    await fireEvent.click(screen.getByRole("button", { name: "Amount" }));
    await screen.findByText("Visa");
    expect(finds.at(-1)).toMatchObject({ sort: "amount", descending: false });
  });
});
