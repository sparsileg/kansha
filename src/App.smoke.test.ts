import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";

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
      scheduleAutoEnter: () => ok({ entered: [], failed: [] }),
      scheduleList: () => ok([]),
      scheduleDueList: () => ok([]),
      scheduleReviewList: () => ok([]),
      calendarOccurrences: () => ok([]),
      calendarProjection: () => ok([]),
    },
  };
});

import App from "./App.svelte";
import { registerState } from "./lib/state/register.svelte";
import { settingsState } from "./lib/state/settings.svelte";
import { viewState } from "./lib/state/view.svelte";

const account = () => screen.findByRole("button", { name: /Savings/ });

beforeEach(() => {
  settingsState.setAccountPanelOpen(true);
  settingsState.setHome("dashboard");
  registerState.accountId = null;
  viewState.reset();
});

describe("App shell", () => {
  it("shows the menu bar, the quick-jump bar, and the account list", async () => {
    render(App);
    for (const m of ["File", "Edit", "Tools", "Reports", "Help"]) {
      expect(screen.getByRole("button", { name: m })).toBeTruthy();
    }
    expect(screen.getByRole("button", { name: "Home" })).toBeTruthy();
    expect(screen.getByRole("searchbox", { name: "Search" })).toHaveProperty("disabled", false);
    await account();
  });

  it("selecting an account shows its register and status line", async () => {
    render(App);
    await fireEvent.click(await account());
    await waitFor(() => expect(screen.getByRole("grid")).toBeTruthy());
    expect(screen.getByLabelText("Payment")).toBeTruthy();
    expect(screen.getByText("0 transactions")).toBeTruthy();
    // Current and Ending sit at the bottom right, not in the header.
    expect(screen.getByText("Ending")).toBeTruthy();
  });

  it("switches views from the menus and the quick-jump bar", async () => {
    render(App);
    await fireEvent.click(await account());
    await waitFor(() => expect(screen.getByRole("grid")).toBeTruthy());
    await fireEvent.click(screen.getByRole("button", { name: "Home" }));
    await waitFor(() => expect(screen.queryByRole("grid")).toBeNull());
    await fireEvent.click(screen.getByRole("button", { name: "Tools" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Categories" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "Categories", selected: true })).toBeTruthy());
    await fireEvent.click(screen.getByRole("tab", { name: "Tags" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "Tags", selected: true })).toBeTruthy());
  });

  it("opens the reminders list and the calendar", async () => {
    render(App);
    await account();
    await fireEvent.click(screen.getByRole("button", { name: /^Reminders/ }));
    await waitFor(() => expect(screen.getByText("No scheduled transactions yet.")).toBeTruthy());
    expect(screen.getByRole("heading", { name: "Reminders" })).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Calendar" }));
    await waitFor(() => expect(screen.getByRole("grid", { name: "Month" })).toBeTruthy());
    expect(screen.getByText("September 2026")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Next month" }));
    await waitFor(() => expect(screen.getByText("October 2026")).toBeTruthy());
  });

  it("gets back to the current account from another view by clicking it in the list", async () => {
    render(App);
    await fireEvent.click(await account());
    await waitFor(() => expect(screen.getByRole("grid")).toBeTruthy());
    await fireEvent.click(screen.getByRole("button", { name: "Calendar" }));
    await waitFor(() => expect(screen.getByRole("grid", { name: "Month" })).toBeTruthy());
    await fireEvent.click(await account());
    await waitFor(() => expect(screen.getByLabelText("Payment")).toBeTruthy());
  });

  it("opens Settings and the Accounts list from the menus", async () => {
    render(App);
    await account();
    await fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: /Settings/ }));
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    await fireEvent.click(within(dialog).getAllByRole("button", { name: "Close" }).at(-1)!);
    await fireEvent.click(screen.getByRole("button", { name: "Tools" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Accounts" }));
    expect(await screen.findByRole("heading", { name: "Accounts" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Edit Savings" })).toBeTruthy();
  });

  it("starts on the home screen setting", async () => {
    settingsState.setHome("calendar");
    render(App);
    await waitFor(() => expect(screen.getByRole("grid", { name: "Month" })).toBeTruthy());
  });
});
