import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return {
    ...real,
    commands: {
      today: vi.fn(),
      accountList: vi.fn(),
      accountBalances: vi.fn(),
      categoryList: vi.fn(),
      tagList: vi.fn(),
      payeeList: vi.fn(),
      taxLineList: vi.fn(),
      registerQuery: vi.fn(),
      registerSummary: vi.fn(),
    },
  };
});

import { commands } from "../api";
import { listsState } from "./lists.svelte";
import { registerState } from "./register.svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const c = vi.mocked(commands, true);

const page = (n: number, total = n) => ({
  rows: Array.from({ length: n }, (_, i) => ({ txn_id: i })) as never[],
  total,
  today: "2026-09-24",
});
const summary = {
  current: "1.00",
  cleared: "1.00",
  ending: "1.00",
  available_credit: null,
};

beforeEach(() => {
  vi.clearAllMocks();
  c.registerQuery.mockImplementation(() => ok(page(3)));
  c.registerSummary.mockImplementation(() => ok(summary));
  c.accountBalances.mockImplementation(() => ok([]));
});

describe("listsState", () => {
  it("loads everything and flags an empty book", async () => {
    c.today.mockReturnValue(ok("2026-09-24"));
    c.accountList.mockReturnValue(ok([]));
    c.accountBalances.mockReturnValue(ok([]));
    c.categoryList.mockReturnValue(ok([]));
    c.tagList.mockReturnValue(ok([]));
    c.payeeList.mockReturnValue(ok([]));
    c.taxLineList.mockReturnValue(ok([]));
    expect(listsState.isEmptyBook).toBe(false);
    await listsState.loadAll();
    expect(listsState.today).toBe("2026-09-24");
    expect(listsState.isEmptyBook).toBe(true);
    expect(listsState.error).toBeNull();
  });

  it("records an error and still marks loaded", async () => {
    c.today.mockReturnValue(
      Promise.resolve({
        status: "error" as const,
        error: { kind: "invalid" as const, message: "boom" },
      }),
    );
    c.accountList.mockReturnValue(ok([]));
    c.accountBalances.mockReturnValue(ok([]));
    c.categoryList.mockReturnValue(ok([]));
    c.tagList.mockReturnValue(ok([]));
    c.payeeList.mockReturnValue(ok([]));
    c.taxLineList.mockReturnValue(ok([]));
    await listsState.loadAll();
    expect(listsState.error).toBe("boom");
    expect(listsState.loaded).toBe(true);
  });
});

describe("registerState", () => {
  it("opens date-ascending with every row, in one query", async () => {
    await registerState.open(7);
    expect(c.registerQuery).toHaveBeenCalledTimes(1);
    expect(c.registerQuery).toHaveBeenLastCalledWith(
      expect.objectContaining({
        account: 7,
        sort: "date",
        descending: false,
        limit: null,
        offset: 0,
      }),
    );
    expect(registerState.rows).toHaveLength(3);
  });

  it("a filter reloads every matching row; clear-all empties filters", async () => {
    await registerState.open(7);
    await registerState.setFilters({ text: "rent" });
    expect(registerState.filtered).toBe(true);
    expect(c.registerQuery).toHaveBeenLastCalledWith(
      expect.objectContaining({ text: "rent", limit: null, offset: 0 }),
    );
    await registerState.clearFilters();
    expect(registerState.filtered).toBe(false);
  });

  it("sortBy toggles direction and starts every new column ascending", async () => {
    await registerState.open(7);
    expect(registerState.descending).toBe(false);
    await registerState.sortBy("date");
    expect(registerState.descending).toBe(true);
    await registerState.sortBy("payee");
    expect(registerState.sort).toBe("payee");
    expect(registerState.descending).toBe(false);
    await registerState.sortBy("date");
    expect(registerState.descending).toBe(false);
  });

  it("opens scrolled to the bottom so the newest rows are next to the entry row", async () => {
    c.registerQuery.mockImplementation(() => ok(page(250)));
    await registerState.open(7);
    expect(registerState.rows).toHaveLength(250);
    expect(registerState.scrollToEnd).toBe(true);
  });

  it("goes to a transaction by selecting and revealing it, not by filtering", async () => {
    c.registerQuery.mockImplementation(() => ok(page(250)));
    await registerState.goToTransaction(7, 40);
    expect(registerState.accountId).toBe(7);
    expect(registerState.selected).toBe(40);
    expect(registerState.reveal).toBe(40);
    expect(registerState.scrollToEnd).toBe(false);
    expect(registerState.filtered).toBe(false);
    expect(registerState.rows).toHaveLength(250);
  });

  it("drops a stale response", async () => {
    await registerState.open(7);
    let release!: () => void;
    const slow = new Promise<void>((r) => (release = r));
    c.registerQuery.mockImplementationOnce(async () => {
      await slow;
      return { status: "ok" as const, data: page(1) };
    });
    const first = registerState.setFilters({ text: "a" });
    c.registerQuery.mockImplementationOnce(() => ok(page(2)));
    await registerState.setFilters({ text: "ab" });
    release();
    await first;
    expect(registerState.rows).toHaveLength(2);
  });

  it("refresh reloads the page and balances", async () => {
    await registerState.open(7);
    c.accountBalances.mockClear();
    await registerState.refresh();
    expect(c.accountBalances).toHaveBeenCalledTimes(1);
  });
});
