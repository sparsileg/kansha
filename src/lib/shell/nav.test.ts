import { beforeEach, describe, expect, it, vi } from "vitest";

const open = vi.hoisted(() => vi.fn());
vi.mock("../state/register.svelte", async () => {
  const { registerStub } = await import("../components/shell/registerStub");
  return { registerState: registerStub(open) };
});

import { goHome, openAccount, openStartup, startupChoices } from "./nav";
import { listsState } from "../state/lists.svelte";
import { registerState } from "../state/register.svelte";
import { settingsState } from "../state/settings.svelte";
import { viewState } from "../state/view.svelte";
import { windowState } from "../state/windows.svelte";

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  listsState.accounts = [{ id: 7, name: "Checking", account_type: "checking", group: "banking", status: "open", show_in_list: true, sort_order: 0, investment: null }] as never;
  registerState.accountId = null;
  windowState.reset();
  viewState.reset();
});

describe("openAccount", () => {
  it("opens a different account fresh, but only navigates back to the one already open", async () => {
    await openAccount(7);
    expect(open).toHaveBeenCalledTimes(1);
    expect(viewState.current).toBe("account");
    viewState.navigate("search");
    await openAccount(7);
    expect(open).toHaveBeenCalledTimes(1); // filters and sort are kept
    expect(viewState.current).toBe("account");
  });
});

describe("goHome", () => {
  it("always goes to the dashboard, whatever the startup setting", () => {
    settingsState.setStartup("calendar");
    viewState.navigate("search");
    goHome();
    expect(viewState.current).toBe("dashboard");
    expect(windowState.wins).toHaveLength(0);
  });
});

describe("openStartup", () => {
  it("opens what the startup setting says", async () => {
    settingsState.setStartup("calendar");
    await openStartup();
    expect(windowState.shownKind).toBe("calendar");
    settingsState.setStartup("scheduled");
    await openStartup();
    expect(windowState.shownKind).toBe("scheduled");
    expect(windowState.wins.map((w) => w.kind)).toEqual(["calendar", "scheduled"]);
    settingsState.setStartup("calendar");
    await openStartup();
    expect(windowState.wins).toHaveLength(2); // one Calendar window, shown again
    settingsState.setStartup("accounts");
    await openStartup();
    expect(windowState.shownKind).toBe("accounts");
    settingsState.setStartup("investments");
    await openStartup();
    expect(viewState.current).toBe("investments");
    settingsState.setStartup("account:7");
    await openStartup();
    expect(viewState.current).toBe("account");
    expect(open).toHaveBeenCalledWith(7);
  });

  it("falls back to the dashboard for an account that is gone or a setting it cannot open", async () => {
    for (const to of ["account:99", "nonsense", "search", "reconcile", "window"]) {
      viewState.navigate("manage");
      settingsState.setStartup(to);
      await openStartup();
      expect(viewState.current, to).toBe("dashboard");
    }
    expect(windowState.wins).toHaveLength(0);
  });
});

describe("startupChoices", () => {
  it("lists the views and panels, then every account, each one opening", async () => {
    listsState.accounts = [
      ...listsState.accounts,
      { id: 8, name: "Visa", account_type: "credit_card", group: "banking", status: "open", show_in_list: true, sort_order: 1, investment: null },
    ] as never;
    const choices = startupChoices();
    expect(choices.map((c) => c.label)).toEqual([
      "Dashboard",
      "Investments",
      "Reminders",
      "Calendar",
      "Accounts",
      "Account: Checking",
      "Account: Visa",
    ]);
    for (const c of choices) {
      windowState.reset();
      viewState.reset();
      viewState.navigate("manage");
      settingsState.setStartup(c.value);
      await openStartup();
      const went = windowState.shownKind ?? viewState.current;
      expect(went, c.value).not.toBe("manage");
      if (c.value !== "dashboard") expect(went, c.value).not.toBe("dashboard");
    }
  });
});
