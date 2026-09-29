import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/svelte";

const info = {
  folder: "/home/u/Downloads",
  folder_missing_now: false,
  status: { last_at: null, last_verified_at: null },
};

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
  return {
    ...real,
    commands: {
      backupInfo: () => ok(info),
      settingsSet: (s: unknown) => ok(s),
      pickFolder: () => Promise.resolve(null),
    },
  };
});

import SettingsModal from "./SettingsModal.svelte";
import { bookSettings } from "../state/booksettings.svelte";
import { dialogState } from "../state/dialogs.svelte";

beforeEach(() => {
  cleanup();
  bookSettings.reset();
  dialogState.settings = true;
  dialogState.verify = false;
  dialogState.passphrase = false;
  dialogState.dbKey = false;
});

describe("SettingsModal backups", () => {
  it("no longer carries the note about where a backup folder should be", () => {
    render(SettingsModal);
    expect(screen.queryByText(/A folder on this computer/)).toBeNull();
  });

  it("shows the folder path and its buttons under the Backup folder label", () => {
    render(SettingsModal);
    const field = screen.getByText("Backup folder").closest(".folder")!;
    expect(within(field as HTMLElement).getByText("Downloads (default)")).toBeTruthy();
    expect(within(field as HTMLElement).getByRole("button", { name: "Browse…" })).toBeTruthy();
  });

  it("offers the three backup tools in one dropdown, opened with Apply", async () => {
    render(SettingsModal);
    const tool = screen.getByLabelText("Backup tools") as HTMLSelectElement;
    expect([...tool.options].map((o) => o.textContent)).toEqual([
      "Verify backup…",
      "Change backup passphrase…",
      "Show database key…",
    ]);
    expect(screen.queryByRole("button", { name: "Show database key…" })).toBeNull();
    await fireEvent.change(tool, { target: { value: "dbKey" } });
    await fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(dialogState.dbKey).toBe(true);
    expect(dialogState.settings).toBe(false);
  });

  it("Apply with the first choice opens the verify dialog", async () => {
    render(SettingsModal);
    await fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(dialogState.verify).toBe(true);
    expect(dialogState.settings).toBe(false);
  });
});
