import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";

const calls = vi.hoisted(() => ({ list: [] as string[], passphrase: "" }));

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
  const side = (name: string, txns: number, value: string) => ({ name, account_type: "checking", txns, value });
  return {
    ...real,
    commands: {
      pickBackupFile: () => Promise.resolve("/b/kansha-backup-2026-09-28T10-00-00Z-close.zip"),
      backupManifest: () =>
        ok({ format: 1, created_at: "2026-09-28T10:00:00Z", app_version: "0.1.0", schema_version: 4, kind: "close" }),
      restoreOpen: (_: string, p: string) => {
        calls.passphrase = p;
        calls.list.push("open");
        if (p !== "right") {
          return Promise.resolve({ status: "error" as const, error: { kind: "wrong_passphrase" as const, message: "the passphrase is not correct" } });
        }
        return ok({
          manifest: { format: 1, created_at: "2026-09-28T10:00:00Z", app_version: "0.1.0", schema_version: 4, kind: "close" },
          integrity: { issues: [] },
          comparison: {
            backup_created_at: "2026-09-28T10:00:00Z",
            backup_last_change: "2026-09-28T09:59:00Z",
            current_last_change: "2026-09-29T08:00:00Z",
            rows: [
              { account: 1, backup: side("Checking", 10, "100.00"), current: side("Checking", 11, "112.34"), differs: true },
              { account: 2, backup: side("Savings", 3, "5000.00"), current: side("Savings", 3, "5000.00"), differs: false },
              { account: 3, backup: side("Old Card", 1, "-5.00"), current: null, differs: true },
              { account: 4, backup: null, current: side("New Brokerage", 2, "900.00"), differs: true },
            ],
          },
        });
      },
      restoreApply: () => {
        calls.list.push("apply");
        return ok(null);
      },
      restoreCancel: () => {
        calls.list.push("cancel");
        return ok(null);
      },
    },
  };
});

import RestoreModal from "./RestoreModal.svelte";

beforeEach(() => {
  cleanup();
  calls.list = [];
});

async function openBackup(passphrase: string) {
  await fireEvent.click(screen.getByRole("button", { name: "Choose backup file…" }));
  await screen.findByText("2026-09-28T10:00:00Z");
  await fireEvent.input(screen.getByLabelText(/Passphrase this backup was made with/), { target: { value: passphrase } });
  await fireEvent.click(screen.getByRole("button", { name: "Open backup" }));
}

describe("Restore window (BAK-070, BAK-075)", () => {
  it("shows the manifest, refuses a wrong passphrase, and compares before replacing", async () => {
    const done = vi.fn();
    render(RestoreModal, { onclose: vi.fn(), ondone: done });
    await openBackup("wrong");
    expect(await screen.findByText("the passphrase is not correct")).toBeTruthy();
    expect(calls.list).toEqual(["open"]);

    await fireEvent.input(screen.getByLabelText(/Passphrase this backup was made with/), { target: { value: "right" } });
    await fireEvent.click(screen.getByRole("button", { name: "Open backup" }));
    expect(await screen.findByText("2026-09-29T08:00:00Z")).toBeTruthy();
    // Differences: a symbol and bold, not color.
    const marks = screen.getAllByText("≠");
    expect(marks).toHaveLength(3);
    expect(screen.getByText("112.34").classList.contains("b")).toBe(true);
    expect(screen.getAllByText("5,000.00")[0].classList.contains("b")).toBe(false);
    expect(screen.getByText("Only in the backup")).toBeTruthy();
    expect(screen.getByText("Old Card")).toBeTruthy();
    expect(screen.getByText(/Only in the current book/)).toBeTruthy();
    expect(screen.getByText("New Brokerage")).toBeTruthy();
    // The filter hides rows that match.
    await fireEvent.click(screen.getByLabelText("Show only rows that differ"));
    expect(screen.queryByText("Savings")).toBeNull();
    expect(done).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole("button", { name: "Restore" }));
    await vi.waitFor(() => expect(done).toHaveBeenCalled());
    expect(calls.list).toEqual(["open", "open", "apply"]);
  });

  it("Cancel after comparing drops the opened backup", async () => {
    const close = vi.fn();
    render(RestoreModal, { onclose: close, ondone: vi.fn() });
    await openBackup("right");
    await screen.findByText("Only in the backup");
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(close).toHaveBeenCalled();
    expect(calls.list).toEqual(["open", "cancel"]);
  });
});
