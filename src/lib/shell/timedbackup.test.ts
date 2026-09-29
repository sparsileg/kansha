import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return { ...real, commands: { backupTimedDue: vi.fn(), backupTimedRun: vi.fn() } };
});

import { commands } from "../api";
import { statusState } from "../state/status.svelte";
import {
  CHECK_MS,
  RETRY_AFTER_CHECKS,
  startTimedBackups,
  stopTimedBackups,
  timedBackupCheck,
} from "./timedbackup";

const c = vi.mocked(commands, true);
const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const done = { path: "/b/kansha-20260929-183012Z-timeout.zip", folder_missing: false, integrity_issues: 0 };

beforeEach(() => {
  vi.clearAllMocks();
  statusState.clear();
  stopTimedBackups();
});
afterEach(() => {
  stopTimedBackups();
  vi.useRealTimers();
});

describe("timed backups", () => {
  it("does nothing when none is due", async () => {
    c.backupTimedDue.mockImplementation(() => ok(false) as never);
    await timedBackupCheck();
    expect(c.backupTimedRun).not.toHaveBeenCalled();
    expect(statusState.message).toBeNull();
  });

  it("says it is starting before the backup, and that it finished after", async () => {
    c.backupTimedDue.mockImplementation(() => ok(true) as never);
    const seen: (string | undefined)[] = [];
    c.backupTimedRun.mockImplementation(() => {
      seen.push(statusState.message?.text);
      return ok(done) as never;
    });
    await timedBackupCheck();
    expect(seen).toEqual(["Timed backup starting…"]);
    expect(statusState.message).toEqual({
      text: `Timed backup finished: ${done.path}.`,
      kind: "info",
    });
  });

  it("flashes when the folder was missing or the data had problems", async () => {
    c.backupTimedDue.mockImplementation(() => ok(true) as never);
    c.backupTimedRun.mockImplementation(
      () => ok({ ...done, folder_missing: true, integrity_issues: 3 }) as never,
    );
    await timedBackupCheck();
    expect(statusState.message?.kind).toBe("alert");
    expect(statusState.message?.text).toContain("3 problem(s)");
  });

  it("flashes a failure, then waits before trying again", async () => {
    c.backupTimedDue.mockImplementation(() => ok(true) as never);
    c.backupTimedRun.mockImplementation(() => Promise.reject(new Error("disk full")));
    await timedBackupCheck();
    expect(statusState.message).toEqual({ text: "The timed backup failed: disk full", kind: "alert" });
    statusState.clear();
    for (let i = 0; i < RETRY_AFTER_CHECKS; i++) await timedBackupCheck();
    expect(c.backupTimedDue).toHaveBeenCalledTimes(1);
    expect(statusState.message).toBeNull();
    await timedBackupCheck();
    expect(c.backupTimedDue).toHaveBeenCalledTimes(2);
  });

  it("does not start a second backup while one is running", async () => {
    c.backupTimedDue.mockImplementation(() => ok(true) as never);
    let finish: (v: unknown) => void = () => {};
    c.backupTimedRun.mockImplementation(() => new Promise((r) => (finish = r)) as never);
    const first = timedBackupCheck();
    await vi.waitFor(() => expect(c.backupTimedRun).toHaveBeenCalled());
    await timedBackupCheck();
    expect(c.backupTimedDue).toHaveBeenCalledTimes(1);
    finish({ status: "ok", data: done });
    await first;
  });

  it("asks every 30 seconds once started, and starting again adds no second timer", async () => {
    vi.useFakeTimers();
    c.backupTimedDue.mockImplementation(() => ok(false) as never);
    startTimedBackups();
    startTimedBackups();
    await vi.advanceTimersByTimeAsync(CHECK_MS * 2);
    expect(c.backupTimedDue).toHaveBeenCalledTimes(2);
  });
});
