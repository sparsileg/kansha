import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";

const calls = vi.hoisted(() => ({ list: [] as string[], passphrase: "", book: null as string | null }));

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
  const side = (name: string, txns: number, value: string) => ({ name, account_type: "checking", txns, value });
  return {
    ...real,
    commands: {
      pickBackupFile: () => Promise.resolve("/b/kansha-backup-2026-09-28T10-00-00Z-close.zip"),
      backupManifest: () =>
        ok({ format: 1, created_at: "2026-09-28T10:00:00Z", app_version: "0.1.0", schema_version: 4, kind: "close", book: calls.book }),
      restoreOpen: (_: string, p: string) => {
        calls.passphrase = p;
        calls.list.push("open");
        if (p !== "right") {
          return Promise.resolve({ status: "error" as const, error: { kind: "wrong_passphrase" as const, message: "the passphrase is not correct" } });
        }
        return ok({
          manifest: { format: 1, created_at: "2026-09-28T10:00:00Z", app_version: "0.1.0", schema_version: 4, kind: "close", book: calls.book },
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
import { bookState } from "../../state/book.svelte";

beforeEach(() => {
  cleanup();
  calls.list = [];
  calls.book = null;
  bookState.status = {
    state: "locked",
    open: true,
    name: "copycarrytransactions",
    db_path: "/b/copycarrytransactions.db",
    folder: "/b",
    downloads: "/home/u/Downloads",
  };
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

  it("another book's backup warns, and replaces only once ticked", async () => {
    calls.book = "barton2026";
    const done = vi.fn();
    render(RestoreModal, { onclose: vi.fn(), ondone: done });
    await fireEvent.click(screen.getByRole("button", { name: "Choose backup file…" }));
    const warning = await screen.findByText(/You are restoring a backup of the book barton2026/);
    const text = warning.textContent?.replace(/\s+/g, " ");
    expect(text).toContain("replaces the data of the book copycarrytransactions, which keeps its name");
    expect(text).toContain("open barton2026 first (File > Open…), then restore this backup");
    expect(warning.classList.contains("danger")).toBe(true);

    await fireEvent.input(screen.getByLabelText(/Passphrase this backup was made with/), { target: { value: "right" } });
    await fireEvent.click(screen.getByRole("button", { name: "Open backup" }));
    await screen.findByText("Only in the backup");
    // Still warned after comparing; the button names what it replaces.
    expect(screen.getByText(/You are restoring a backup of the book barton2026/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Restore" })).toBeNull();
    const replace = screen.getByRole("button", { name: "Replace copycarrytransactions" }) as HTMLButtonElement;
    expect(replace.disabled).toBe(true);
    await fireEvent.click(screen.getByLabelText("Replace copycarrytransactions with barton2026's data"));
    expect(replace.disabled).toBe(false);
    await fireEvent.click(replace);
    await vi.waitFor(() => expect(done).toHaveBeenCalled());
    expect(calls.list).toEqual(["open", "apply"]);
  });

  it("the same book's backup has no warning or tick box", async () => {
    calls.book = "copycarrytransactions";
    render(RestoreModal, { onclose: vi.fn(), ondone: vi.fn() });
    await openBackup("right");
    await screen.findByText("Only in the backup");
    expect(screen.queryByText(/You are restoring a backup/)).toBeNull();
    expect(screen.queryByLabelText(/with .*'s data/)).toBeNull();
    expect((screen.getByRole("button", { name: "Restore" }) as HTMLButtonElement).disabled).toBe(false);
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
