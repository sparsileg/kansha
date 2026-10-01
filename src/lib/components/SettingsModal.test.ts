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
      settingsSet: vi.fn((s: unknown) => ok(s)),
      pickFolder: () => Promise.resolve(null),
    },
  };
});

import SettingsModal from "./SettingsModal.svelte";
import { bookSettings } from "../state/booksettings.svelte";
import { dialogState } from "../state/dialogs.svelte";

/** Show one settings category in the card. */
async function show(name: string) {
  await fireEvent.change(screen.getByLabelText("Settings category"), { target: { value: name } });
}

beforeEach(() => {
  cleanup();
  bookSettings.reset();
  dialogState.settings = true;
  dialogState.verify = false;
  dialogState.passphrase = false;
  dialogState.dbKey = false;
});

describe("SettingsModal backups", () => {
  it("no longer carries the note about where a backup folder should be", async () => {
    render(SettingsModal);
    await show("backups");
    expect(screen.queryByText(/A folder on this computer/)).toBeNull();
  });

  it("shows the folder path and its buttons under the Backup folder label", async () => {
    render(SettingsModal);
    await show("backups");
    const field = screen.getByText("Backup folder").closest(".folder")!;
    expect(within(field as HTMLElement).getByText("Downloads (default)")).toBeTruthy();
    expect(within(field as HTMLElement).getByRole("button", { name: "Browse…" })).toBeTruthy();
  });

  it("offers the three backup tools in one dropdown, opened with Apply", async () => {
    render(SettingsModal);
    await show("backups");
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
    await show("backups");
    await fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(dialogState.verify).toBe(true);
    expect(dialogState.settings).toBe(false);
  });
});

describe("SettingsModal categories", () => {
  it("lists the categories in a dropdown and shows one card at a time", async () => {
    render(SettingsModal);
    const pick = screen.getByLabelText("Settings category") as HTMLSelectElement;
    expect([...pick.options].map((o) => o.textContent)).toEqual([
      "Interface",
      "Data",
      "Investments",
      "Register",
      "Notifications",
      "Backups",
    ]);
    expect(screen.getByLabelText("Date format")).toBeTruthy();
    expect(screen.queryByLabelText("Integrity check at startup")).toBeNull();
    await show("data");
    expect(screen.getByLabelText("Integrity check at startup")).toBeTruthy();
    expect(screen.queryByLabelText("Date format")).toBeNull();
  });

  it("holds edits until OK, and Cancel drops them", async () => {
    const { commands } = await import("../api");
    vi.mocked(commands.settingsSet).mockClear();
    render(SettingsModal);
    await show("register");
    await fireEvent.click(screen.getByLabelText("Capitalize payees and categories"));
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(commands.settingsSet).not.toHaveBeenCalled();
    expect(dialogState.settings).toBe(false);

    dialogState.settings = true;
    cleanup();
    render(SettingsModal);
    await show("register");
    await fireEvent.click(screen.getByLabelText("Capitalize payees and categories"));
    await fireEvent.click(screen.getByRole("button", { name: "OK" }));
    const sent = vi.mocked(commands.settingsSet).mock.calls.at(-1)?.[0] as { capitalize_names: boolean };
    expect(sent.capitalize_names).toBe(true);
    expect(dialogState.settings).toBe(false);
  });

  it("has the notification settings", async () => {
    render(SettingsModal);
    await show("notifications");
    expect(screen.getByLabelText(/out-of-date transactions/)).toBeTruthy();
    expect(screen.getByLabelText("A check number is reused")).toBeTruthy();
    expect(screen.getByLabelText("Save a transaction after changing it")).toBeTruthy();
  });
});

describe("SettingsModal investments", () => {
  it("sets the lot method for new accounts (SET-040) and allows price download (PRC-040)", async () => {
    const { commands } = await import("../api");
    render(SettingsModal);
    await show("investments");
    const lot = screen.getByLabelText("Lot method for new investment accounts") as HTMLSelectElement;
    expect(lot.value).toBe("fifo");
    await fireEvent.change(lot, { target: { value: "hifo" } });
    const dl = screen.getByLabelText("Allow price download (internet)") as HTMLInputElement;
    expect(dl.checked).toBe(false);
    await fireEvent.click(dl);
    await fireEvent.click(screen.getByRole("button", { name: "OK" }));
    const sent = vi.mocked(commands.settingsSet).mock.calls.at(-1)?.[0] as { default_lot_method: string; price_download: boolean };
    expect(sent.default_lot_method).toBe("hifo");
    expect(sent.price_download).toBe(true);
  });
});
