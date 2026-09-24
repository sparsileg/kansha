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
    await listsState.loadAll();
    expect(listsState.error).toBe("boom");
    expect(listsState.loaded).toBe(true);
  });
});

describe("registerState", () => {
  it("opens newest-first with paging", async () => {
    await registerState.open(7);
    expect(c.registerQuery).toHaveBeenLastCalledWith(
      expect.objectContaining({
        account: 7,
        sort: "date",
        descending: true,
        limit: 100,
        offset: 0,
      }),
    );
    expect(registerState.rows).toHaveLength(3);
  });

  it("filter change resets to page 0; clear-all empties filters", async () => {
    await registerState.open(7);
    c.registerQuery.mockImplementation(() => ok(page(100, 350)));
    await registerState.reload();
    await registerState.goToPage(2);
    expect(c.registerQuery).toHaveBeenLastCalledWith(
      expect.objectContaining({ offset: 200 }),
    );
    await registerState.setFilters({ text: "rent" });
    expect(registerState.pageIndex).toBe(0);
    expect(registerState.filtered).toBe(true);
    expect(c.registerQuery).toHaveBeenLastCalledWith(
      expect.objectContaining({ text: "rent", offset: 0 }),
    );
    await registerState.clearFilters();
    expect(registerState.filtered).toBe(false);
  });

  it("sortBy toggles direction and starts non-date columns ascending", async () => {
    await registerState.open(7);
    await registerState.sortBy("date");
    expect(registerState.descending).toBe(false);
    await registerState.sortBy("payee");
    expect(registerState.sort).toBe("payee");
    expect(registerState.descending).toBe(false);
    await registerState.sortBy("date");
    expect(registerState.descending).toBe(true);
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
