import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const search = vi.hoisted(() => vi.fn());
const goTo = vi.hoisted(() => vi.fn());

vi.mock("../lib/api", async (orig) => {
  const real = await orig<typeof import("../lib/api")>();
  return { ...real, commands: { searchTransactions: (q: unknown) => search(q) } };
});
vi.mock("../lib/state/register.svelte", () => ({
  registerState: { goToTransaction: (...a: unknown[]) => goTo(...a) },
}));

import Search from "./Search.svelte";
import { listsState } from "../lib/state/lists.svelte";
import { viewState } from "../lib/state/view.svelte";

const hit = (txn: number, account: number, over: Record<string, unknown> = {}) => ({
  txn_id: txn, account, date: "2026-02-01", payee_name: "Costco", memo: "towels",
  status: "posted", amount: "-250.00", category: "--Split--", ...over,
});

beforeEach(() => {
  cleanup();
  vi.clearAllMocks();
  viewState.reset();
  listsState.accounts = [{ id: 1, name: "Checking" }, { id: 2, name: "Savings" }] as never;
  search.mockImplementation(() => ok({ rows: [hit(7, 1)], total: 1 }));
});

describe("Search view", () => {
  it("asks for the text in the URL-like params and lists the matches, one line each", async () => {
    viewState.navigate("search", { q: "costco" });
    render(Search);
    await screen.findByText("1 match.");
    expect(search).toHaveBeenCalledWith({ text: "costco", account: null, limit: 200 });
    const row = screen.getByRole("button", { name: /Costco/ });
    for (const t of ["Costco", "Checking", "--Split--", "towels", "250.00"]) {
      expect(row.textContent).toContain(t);
    }
  });

  it("clicking a match opens that account on the transaction", async () => {
    viewState.navigate("search", { q: "costco" });
    render(Search);
    await fireEvent.click(await screen.findByRole("button", { name: /Costco/ }));
    await waitFor(() => expect(goTo).toHaveBeenCalledWith(1, 7, "2026-02-01"));
    expect(viewState.current).toBe("account");
  });

  it("says how many there are when the list is cut, and marks a void", async () => {
    search.mockImplementation(() => ok({ rows: [hit(7, 1, { status: "void" })], total: 450 }));
    viewState.navigate("search", { q: "costco" });
    render(Search);
    await screen.findByText(/450 matches; showing the newest 1/);
    expect(screen.getByRole("button", { name: /\(void\)/ })).toBeTruthy();
  });

  it("no matches, and an empty search, say so", async () => {
    search.mockImplementation(() => ok({ rows: [], total: 0 }));
    viewState.navigate("search", { q: "zzz" });
    render(Search);
    await screen.findByText("No matches for “zzz”.");
    cleanup();
    viewState.navigate("search", { q: "" });
    render(Search);
    expect(screen.getByText(/Type in the search box/)).toBeTruthy();
  });

  it("limited to one account: says so and offers all accounts", async () => {
    viewState.navigate("search", { q: "rent", account: 2 });
    render(Search);
    await screen.findByText(/in Savings/);
    expect(search).toHaveBeenCalledWith({ text: "rent", account: 2, limit: 200 });
    await fireEvent.click(screen.getByRole("button", { name: "Search all accounts" }));
    await waitFor(() => expect(search).toHaveBeenLastCalledWith({ text: "rent", account: null, limit: 200 }));
    expect(within(document.body).queryByRole("button", { name: "Search all accounts" })).toBeNull();
  });
});
