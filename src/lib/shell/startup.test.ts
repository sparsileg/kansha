import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return { ...real, commands: { integrityCheck: vi.fn(), backupNow: vi.fn() } };
});

import { commands } from "../api";
import { dialogState } from "../state/dialogs.svelte";
import { statusState } from "../state/status.svelte";
import { runAction, backUpNow } from "./actions";
import { autoIntegrityCheck } from "./startup";

const c = vi.mocked(commands, true);
const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const issue = { check: "sum", table: "txn", id: 3, detail: "does not balance" };

beforeEach(() => {
  vi.clearAllMocks();
  statusState.clear();
  dialogState.integrity = false;
  dialogState.integrityReport = null;
});

describe("automatic integrity check", () => {
  it("a clean result is a status note, not a window", async () => {
    c.integrityCheck.mockImplementation(() => ok({ issues: [] }) as never);
    await autoIntegrityCheck();
    expect(dialogState.integrity).toBe(false);
    expect(statusState.message).toEqual({
      text: "Integrity check found no problems with the data.",
      kind: "info",
    });
  });

  it("problems open the window with the report already in hand", async () => {
    const report = { issues: [issue] };
    c.integrityCheck.mockImplementation(() => ok(report) as never);
    await autoIntegrityCheck();
    expect(dialogState.integrity).toBe(true);
    expect(dialogState.integrityReport).toEqual(report);
    expect(statusState.message).toBeNull();
  });

  it("a check that cannot run flashes an alert", async () => {
    c.integrityCheck.mockImplementation(() =>
      Promise.resolve({ status: "error" as const, error: { kind: "invalid", message: "locked" } }) as never,
    );
    await autoIntegrityCheck();
    expect(statusState.message?.kind).toBe("alert");
    expect(statusState.message?.text).toContain("could not run");
  });

  it("File > Integrity Check always opens the window and runs its own check", () => {
    dialogState.integrityReport = { issues: [] } as never;
    runAction("file.integrity");
    expect(dialogState.integrity).toBe(true);
    expect(dialogState.integrityReport).toBeNull();
  });
});

describe("Back Up Now", () => {
  it("says where it went, as a note", async () => {
    c.backupNow.mockImplementation(
      () => ok({ path: "/b/x.zip", folder_missing: false, integrity_issues: 0 }) as never,
    );
    await backUpNow();
    expect(statusState.message).toEqual({ text: "Backed up to /b/x.zip.", kind: "info" });
  });

  it("flashes when the folder is missing or the data has problems", async () => {
    c.backupNow.mockImplementation(
      () => ok({ path: "/d/x.zip", folder_missing: true, integrity_issues: 2 }) as never,
    );
    await backUpNow();
    expect(statusState.message?.kind).toBe("alert");
    expect(statusState.message?.text).toContain("folder is missing");
    expect(statusState.message?.text).toContain("2 problem(s)");
  });

  it("flashes a failure", async () => {
    c.backupNow.mockImplementation(() => Promise.reject(new Error("disk full")));
    await backUpNow();
    expect(statusState.message).toEqual({ text: "The backup failed: disk full", kind: "alert" });
  });
});
