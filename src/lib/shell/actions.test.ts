import { beforeEach, describe, expect, it, vi } from "vitest";

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });
const fail = (kind: string, message: string) => Promise.resolve({ status: "error" as const, error: { kind, message } });

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return {
    ...real,
    commands: {
      undoStatus: vi.fn(),
      undoApply: vi.fn(),
      pricesDownload: vi.fn(),
      scheduleList: () => ok([]),
      scheduleDueList: () => ok([]),
      scheduleReviewList: () => ok([]),
      accountBalances: () => ok([]),
      securityList: () => ok([]),
    },
  };
});

import { commands } from "../api";
import { confirmState } from "../state/confirm.svelte";
import { statusState } from "../state/status.svelte";
import { downloadPrices, isCurrent, runAction, undoLast } from "./actions";
import { windowState } from "../state/windows.svelte";
import { reportState } from "../state/reports.svelte";
import { viewState } from "../state/view.svelte";
import type { SavedReport } from "../types/bindings";

const c = vi.mocked(commands, true);

beforeEach(() => {
  vi.clearAllMocks();
  statusState.clear();
});

describe("Edit > Undo (UI-060)", () => {
  it("says when there is nothing to undo", async () => {
    c.undoStatus.mockImplementation(() => ok(null));
    await undoLast();
    expect(c.undoApply).not.toHaveBeenCalled();
    expect(statusState.message?.text).toBe("There is nothing to undo.");
  });

  it("undoes and names what it undid", async () => {
    c.undoStatus.mockImplementation(() => ok("Delete"));
    c.undoApply.mockImplementation(() => ok(12));
    await undoLast();
    expect(c.undoApply).toHaveBeenCalledWith(false);
    expect(statusState.message?.text).toBe("Undone: Delete.");
  });

  it("asks before undoing a change to a reconciled transaction", async () => {
    c.undoStatus.mockImplementation(() => ok("Edit"));
    c.undoApply.mockImplementation((confirmed: boolean) =>
      confirmed ? ok(12) : (fail("confirmation_required", "Undoing this changes a reconciled transaction. Undo anyway?") as never),
    );
    const ask = vi.spyOn(confirmState, "ask").mockResolvedValue(true);
    await undoLast();
    expect(ask).toHaveBeenCalled();
    expect(c.undoApply).toHaveBeenLastCalledWith(true);
    ask.mockRestore();
  });
});

describe("Download Prices (PRC-040)", () => {
  it("reports the prices stored and flashes the tickers that got none", async () => {
    c.pricesDownload.mockImplementation(() => ok({ stored: 3, failed: [{ ticker: "OLDX", reason: "No data found" }] }));
    await downloadPrices("2026-06-12");
    expect(c.pricesDownload).toHaveBeenLastCalledWith("2026-06-12");
    expect(statusState.message).toEqual({ text: "Downloaded 3 prices. No price for OLDX (No data found).", kind: "alert" });
  });

  it("says why when download is off", async () => {
    c.pricesDownload.mockImplementation(() => fail("invalid", "price download is off; turn it on in Settings") as never);
    await downloadPrices("2026-06-12");
    expect(statusState.message?.text).toMatch(/turn it on in Settings/);
    expect(statusState.message?.kind).toBe("alert");
  });
});

describe("Tools > Investments", () => {
  it("opens the investments view, as the nav bar button does", () => {
    runAction("tools.investments");
    expect(windowState.shownKind).toBe("investments");
    expect(isCurrent("tools.investments")).toBe(true);
    expect(isCurrent("view.investments")).toBe(true);
  });
});

describe("Tools > Insights (INS-010)", () => {
  it("opens the Insights view at its first tab, as the nav bar's Insights does", () => {
    viewState.reset();
    viewState.navigate("manage", { tab: "tags" });
    runAction("tools.insights");
    expect(viewState.current).toBe("insights");
    expect(viewState.params).toEqual({});
    expect(isCurrent("tools.insights")).toBe(true);
  });
});

describe("Reports > Saved Reports (RPT-020)", () => {
  it("a saved report's menu item opens that report", async () => {
    const r = { id: 4, name: "Mine", folder: 1, settings: {} } as unknown as SavedReport;
    reportState.savedList = [r];
    const open = vi.spyOn(reportState, "openSaved").mockResolvedValue({} as never);
    runAction("saved:4");
    expect(open).toHaveBeenCalledWith(r);
    open.mockRestore();
  });
});
