import { beforeEach, describe, expect, it, vi } from "vitest";

const open = vi.hoisted(() => vi.fn());
vi.mock("../state/register.svelte", async () => {
  const { registerStub } = await import("../components/shell/registerStub");
  return { registerState: registerStub(open) };
});

import { arrowTitle, canGo, go } from "./history";
import { openAccount } from "./nav";
import { openPanel } from "./panels";
import { listsState } from "../state/lists.svelte";
import { registerState } from "../state/register.svelte";
import { reportState } from "../state/reports.svelte";
import { viewState } from "../state/view.svelte";
import { windowState } from "../state/windows.svelte";
import type { ReportSettings } from "../types/bindings";

const account = (id: number, name: string) =>
  ({ id, name, account_type: "checking", group: "banking", status: "open", show_in_list: true, sort_order: id, investment: null }) as never;

beforeEach(() => {
  vi.clearAllMocks();
  listsState.accounts = [account(7, "Checking"), account(8, "Visa")];
  registerState.accountId = null;
  windowState.reset();
  viewState.reset();
});

describe("Back and Forward (UI-025)", () => {
  it("walk the accounts visited, reopening each register", async () => {
    await openAccount(7);
    await openAccount(8);
    expect(arrowTitle(-1)).toBe("Back to Checking");
    expect(canGo(1)).toBe(false);
    expect(arrowTitle(1)).toBe("Forward");
    await go(-1);
    expect(registerState.accountId).toBe(7);
    expect(arrowTitle(-1)).toBe("Back to Insights");
    expect(arrowTitle(1)).toBe("Forward to Visa");
    await go(1);
    expect(registerState.accountId).toBe(8);
    expect(open).toHaveBeenLastCalledWith(8);
  });

  it("skip a deleted account", async () => {
    await openAccount(7);
    await openAccount(8);
    viewState.navigate("search", { q: "x" });
    listsState.accounts = [account(7, "Checking")];
    expect(arrowTitle(-1)).toBe("Back to Checking");
    await go(-1);
    expect(registerState.accountId).toBe(7);
  });

  it("reopen a closed panel; a docked one just shows", async () => {
    openPanel("calendar");
    viewState.navigate("insights");
    expect(arrowTitle(-1)).toBe("Back to Calendar");
    await go(-1);
    expect(windowState.shownKind).toBe("calendar");
    await windowState.close(windowState.shown!);
    expect(windowState.wins).toHaveLength(0);
    expect(arrowTitle(-1)).toBe("Back to Calendar");
    await go(-1);
    expect(windowState.shownKind).toBe("calendar");
    expect(windowState.wins).toHaveLength(1);
  });

  it("reopen a closed report with the settings it had", async () => {
    const settings = { kind: "net_worth", title: "Net Worth", range: { preset: "custom", from: null, to: "2026-01-31" } } as unknown as ReportSettings;
    const inst = await reportState.openWith(settings);
    inst.settings = { ...settings, title: "Mine" };
    inst.saved = null;
    // Changed but never saved: close without asking.
    vi.spyOn(inst, "dirty", "get").mockReturnValue(false);
    expect(await windowState.close(inst.id)).toBe(true);
    expect(reportState.get(inst.id)).toBeUndefined();
    await go(-1);
    const back = reportState.current;
    expect(back).not.toBeNull();
    expect(back!.id).not.toBe(inst.id);
    expect(back!.settings.title).toBe("Mine");
    expect(viewState.params.window).toBe(back!.id);
  });
});
