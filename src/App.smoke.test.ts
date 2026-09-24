import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";

vi.mock("./lib/api", async (orig) => {
  const real = await orig<typeof import("./lib/api")>();
  const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
  const acct = { id: 1, name: "Savings", account_type: "savings", group: "banking", status: "open", show_in_list: true, sort_order: 0, investment: null };
  return {
    ...real,
    commands: {
      appVersion: () => Promise.resolve("x"),
      today: () => ok("2026-09-24"),
      accountList: () => ok([acct]),
      accountBalances: () => ok([{ account: 1, current: "10.00", ending: "10.00" }]),
      categoryList: () => ok([]),
      tagList: () => ok([]),
      payeeList: () => ok([]),
      registerQuery: () => ok({ rows: [], total: 0, today: "2026-09-24" }),
      registerSummary: () => ok({ current: "10.00", cleared: "0.00", ending: "10.00", available_credit: null }),
      payeeSearch: () => ok([]),
    },
  };
});

import App from "./App.svelte";

describe("App smoke", () => {
  it("selecting an account shows its register", async () => {
    render(App);
    const sel = (await screen.findByLabelText("Account")) as HTMLSelectElement;
    await fireEvent.change(sel, { target: { value: "1" } });
    await waitFor(() => expect(screen.getByRole("grid")).toBeTruthy());
    expect(screen.getByLabelText("Payment")).toBeTruthy();
  });

  it("switches views both ways", async () => {
    render(App);
    const sel = (await screen.findByLabelText("Account")) as HTMLSelectElement;
    await fireEvent.change(sel, { target: { value: "1" } });
    await waitFor(() => expect(screen.getByRole("grid")).toBeTruthy());
    await fireEvent.click(screen.getByRole("button", { name: "Dashboard" }));
    await waitFor(() => expect(screen.queryByRole("grid")).toBeNull());
    await fireEvent.click(screen.getByRole("button", { name: /Payees, categories, tags/ }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "Payees" })).toBeTruthy());
  });
});
