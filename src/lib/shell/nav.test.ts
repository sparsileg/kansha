import { beforeEach, describe, expect, it, vi } from "vitest";

const open = vi.hoisted(() => vi.fn());
vi.mock("../state/register.svelte", async () => {
  const { registerStub } = await import("../components/shell/registerStub");
  return { registerState: registerStub(open) };
});

import { goHome, openAccount } from "./nav";
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
  it("goes where the home setting says", async () => {
    settingsState.setHome("calendar");
    await goHome();
    expect(windowState.shownKind).toBe("calendar");
    settingsState.setHome("scheduled");
    await goHome();
    expect(windowState.shownKind).toBe("scheduled");
    expect(windowState.wins.map((w) => w.kind)).toEqual(["calendar", "scheduled"]);
    settingsState.setHome("calendar");
    await goHome();
    expect(windowState.wins).toHaveLength(2); // one Calendar window, shown again
    settingsState.setHome("account:7");
    await goHome();
    expect(viewState.current).toBe("account");
    expect(open).toHaveBeenCalledWith(7);
  });

  it("falls back to the dashboard for an account that is gone or an unknown setting", async () => {
    viewState.navigate("search");
    settingsState.setHome("account:99");
    await goHome();
    expect(viewState.current).toBe("dashboard");
    viewState.navigate("search");
    settingsState.setHome("nonsense");
    await goHome();
    expect(viewState.current).toBe("dashboard");
  });
});
