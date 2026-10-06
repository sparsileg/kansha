import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import type { ImportOptions, ImportPreview, ImportResult } from "../../types/bindings";

const calls = vi.hoisted(() => ({ list: [] as string[], options: [] as unknown[] }));

function preview(errors = 1): ImportPreview {
  return {
    file_name: "all.qif",
    date_order: "mdy",
    date_ambiguous: false,
    first_date: "2025-01-01",
    last_date: "2025-04-07",
    accounts: [
      {
        name: "Checking",
        qif_type: "Bank",
        investment: false,
        defined: true,
        records: 9,
        first_date: "2025-01-01",
        last_date: "2025-02-03",
        total: "1555.68",
        choice: { kind: "create", name: "Checking", account_type: "checking" },
        default_type: "checking",
      },
      {
        name: "Old Account",
        qif_type: "Bank",
        investment: false,
        defined: true,
        records: 1,
        first_date: "2025-02-02",
        last_date: "2025-02-02",
        total: "50.00",
        choice: { kind: "create", name: "Old Account", account_type: "checking" },
        default_type: "checking",
      },
    ],
    categories: [
      { name: "Food", listed: true, kind: "expense", used: 2, total: "184.32", choice: { kind: "create", path: "Food", category_kind: "expense" }, imported: true },
      { name: "Never Used", listed: true, kind: "expense", used: 0, total: "0.00", choice: { kind: "create", path: "Never Used", category_kind: "expense" }, imported: false },
    ],
    tags: [],
    securities: [
      { name: "Sold Fund", symbol: null, qif_type: "", used: 2, prices: 0, choice: { kind: "create", name: "Sold Fund", ticker: null, security_type: "other" }, imported: true },
    ],
    transactions: 22,
    transfers_matched: 5,
    new_payees: 7,
    prices: 2,
    memorized_skipped: 1,
    warnings: [{ line: null, account: "", message: "1 transfer(s) to accounts not imported" }],
    errors: Array.from({ length: errors }, () => ({ line: 210, account: "Brokerage", message: 'investment action "Foo" is not supported' })),
    imported_before: null,
  };
}

function result(committed: boolean): ImportResult {
  return {
    batch: committed ? 3 : null,
    committed,
    dry_run: !committed,
    transactions: 21,
    accounts_created: 4,
    categories_created: 5,
    tax_lines_set: 2,
    tax_codes_unmapped: 1,
    tags_created: 1,
    securities_created: 1,
    securities_hidden: 0,
    payees_created: 7,
    prices: 2,
    errors: [],
    accounts: [{ account: 1, qif_name: "Checking", expected: "1555.68", before: "0.00", after: "1500.00", differs: true }],
  };
}

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
  return {
    ...real,
    commands: {
      pickImportFile: () => Promise.resolve("/q/all.qif"),
      importOpen: () => {
        calls.list.push("open");
        return ok(preview());
      },
      importPreview: (o: ImportOptions) => {
        calls.list.push("preview");
        calls.options.push(JSON.parse(JSON.stringify(o)));
        return ok(preview());
      },
      importRun: (_: ImportOptions, dryRun: boolean) => {
        calls.list.push(dryRun ? "test" : "import");
        return ok(result(!dryRun));
      },
      importCancel: () => {
        calls.list.push("cancel");
        return ok(null);
      },
      importBatches: () => ok([]),
      accountList: () => ok([]),
      accountBalances: () => ok([]),
      categoryList: () => ok([]),
      tagList: () => ok([]),
      payeeList: () => ok([]),
      taxLineList: () => ok([]),
      insightList: () => ok([]),
      spendingCardList: () => ok([]),
      today: () => ok("2026-06-30"),
      securityList: () => ok([]),
      scheduleList: () => ok([]),
      scheduleDueList: () => ok([]),
      scheduleReviewList: () => ok([]),
    },
  };
});

import ImportModal from "./ImportModal.svelte";

beforeEach(() => {
  cleanup();
  calls.list = [];
  calls.options = [];
});

async function open() {
  render(ImportModal, { onclose: vi.fn() });
  await fireEvent.click(screen.getByRole("button", { name: "Choose QIF file…" }));
  await screen.findByText("all.qif");
}

describe("Import from Quicken (MIG-040 … MIG-100)", () => {
  it("previews accounts, totals, and problems; bad records block until left out", async () => {
    await open();
    expect(screen.getByText("1,555.68")).toBeTruthy();
    expect(screen.getByText(/22 \(5 transfers found on both sides/)).toBeTruthy();
    expect(screen.getByText(/"Foo" is not supported/)).toBeTruthy();
    const importButton = screen.getByRole("button", { name: "Import" }) as HTMLButtonElement;
    expect(importButton.disabled).toBe(true);
    // Unused categories are hidden until asked for.
    expect(screen.queryByText("Never Used")).toBeNull();
    await fireEvent.click(screen.getByLabelText(/Show the 1 the imported transactions do not use/));
    expect(screen.getByText("Never Used")).toBeTruthy();

    await fireEvent.click(screen.getByLabelText("Import the rest and leave these out"));
    expect(importButton.disabled).toBe(false);
  });

  it("sends the mapping back for a new preview", async () => {
    await open();
    await fireEvent.change(screen.getByLabelText("Import Old Account as"), { target: { value: "skip" } });
    await vi.waitFor(() => expect(calls.list).toContain("preview"));
    const o = calls.options.at(-1) as ImportOptions;
    expect(o.accounts["Old Account"]).toEqual({ kind: "skip" });
  });

  it("Keep shown sends a new security's name, to keep it from being hidden when sold out", async () => {
    await open();
    await fireEvent.click(screen.getByLabelText("Keep Sold Fund shown"));
    await fireEvent.click(screen.getByLabelText("Import the rest and leave these out"));
    await fireEvent.click(screen.getByRole("button", { name: "Test import" }));
    await vi.waitFor(() => expect(calls.list).toContain("test"));
    // The options the next preview sends carry the choice.
    await fireEvent.change(screen.getByLabelText("Import Old Account as"), { target: { value: "skip" } });
    await vi.waitFor(() => expect(calls.options.length).toBeGreaterThan(0));
    expect((calls.options.at(-1) as ImportOptions).show_securities).toEqual(["Sold Fund"]);
    await fireEvent.click(screen.getByLabelText("Keep Sold Fund shown"));
    await fireEvent.change(screen.getByLabelText("Import Old Account as"), { target: { value: "skip" } });
    await vi.waitFor(() => expect((calls.options.at(-1) as ImportOptions).show_securities).toEqual([]));
  });

    it("tests, then imports, and shows each account against the file", async () => {
    await open();
    await fireEvent.click(screen.getByLabelText("Import the rest and leave these out"));
    await fireEvent.click(screen.getByRole("button", { name: "Test import" }));
    expect(await screen.findByText(/Test import: 21 transactions would be imported/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Import" }));
    expect(await screen.findByText(/Imported 21 transactions \(import #3\)/)).toBeTruthy();
    // A difference is a symbol and bold, never color alone.
    expect(screen.getByText("≠")).toBeTruthy();
    expect(screen.getByText("1,500.00").classList.contains("b")).toBe(true);
    expect(screen.getByText(/2 new categories their tax lines; 1 code has no Kansha tax line/)).toBeTruthy();
    expect(calls.list).toEqual(["open", "test", "import"]);
  });
});
