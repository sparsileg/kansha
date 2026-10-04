import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });

const settings = {
  kind: "capital_gains",
  title: "Capital Gains",
  range: { preset: "last_year", from: null, to: null },
  subtotal: "term",
  interval: "none",
  sort: "date",
  sort_desc: false,
  hidden_columns: [],
  cents: true,
  totals_only: false,
  totals_on_heading: null,
  show_zero: false,
  transfers: true,
  accounts: null,
  categories: null,
  payees: null,
  securities: null,
  tags: null,
};

// A small book of folders and saved reports, changed by the commands.
const db = vi.hoisted(() => ({
  folders: [] as { id: number; name: string; permanent: boolean }[],
  reports: [] as { id: number; name: string; folder: number; settings: unknown }[],
  next: 10,
}));

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  const byName = <T extends { name: string }>(xs: T[]) => [...xs].sort((a, b) => a.name.localeCompare(b.name));
  return {
    ...real,
    commands: {
      savedReportList: () => ok(byName(db.reports)),
      reportFolderList: () => ok(byName(db.folders)),
      reportFolderCreate: (name: string) => {
        const f = { id: db.next++, name, permanent: false };
        db.folders.push(f);
        return ok(f);
      },
      reportFolderRename: (id: number, name: string) => {
        const f = db.folders.find((x) => x.id === id)!;
        f.name = name;
        return ok(f);
      },
      reportFolderDelete: (id: number) => {
        db.folders = db.folders.filter((x) => x.id !== id);
        return ok(null);
      },
      savedReportMove: (id: number, folder: number) => {
        const r = db.reports.find((x) => x.id === id)!;
        r.folder = folder;
        return ok(r);
      },
      savedReportUpdate: (id: number, name: string) => {
        const r = db.reports.find((x) => x.id === id)!;
        r.name = name;
        return ok(r);
      },
      savedReportDelete: (id: number) => {
        db.reports = db.reports.filter((x) => x.id !== id);
        return ok(null);
      },
    },
  };
});

import SavedReportsModal from "./SavedReportsModal.svelte";
import { reportState } from "../../state/reports.svelte";
import { confirmState } from "../../state/confirm.svelte";
import type { SavedReport } from "../../types/bindings";

beforeEach(() => {
  db.folders = [{ id: 1, name: "Unfiled", permanent: true }];
  db.reports = [{ id: 1, name: "My gains", folder: 1, settings }];
  db.next = 10;
});

const show = () => render(SavedReportsModal, { onopen: () => {}, onclose: () => {} });
const button = (name: string | RegExp) => screen.getByRole("button", { name });
/** The folder's row and the reports listed under it. */
const folderItem = (name: string) => screen.getByRole("button", { name: new RegExp(`^${name} \\d+$`) }).closest("li")!;

async function newFolder(name: string) {
  await fireEvent.click(button("Create folder"));
  await fireEvent.input(screen.getByLabelText("New folder"), { target: { value: name } });
  await fireEvent.click(button("OK"));
  await screen.findByRole("button", { name: new RegExp(`^${name} \\d+$`) });
}

describe("Manage Saved Reports (RPT-020)", () => {
  it("opens the chosen saved report in a report window", async () => {
    // Typed by a cast: TypeScript would narrow a plain `= null` to null.
    let opened = null as Promise<unknown> | null;
    render(SavedReportsModal, {
      onopen: (r: SavedReport) => (opened = reportState.openSaved(r)),
      onclose: () => {},
    });
    await fireEvent.click(await screen.findByRole("button", { name: /My gains/ }));
    await fireEvent.click(button("Open"));
    const inst = (await opened) as { saved: SavedReport | null; settings: { title: string } };
    expect(inst.saved?.name).toBe("My gains");
    expect(inst.settings.title).toBe("Capital Gains");

    // A rename shows on the open copy.
    await fireEvent.click(button("Rename"));
    await fireEvent.input(screen.getByLabelText("Report name"), { target: { value: "Gains" } });
    await fireEvent.click(button("OK"));
    await vi.waitFor(() => expect(inst.saved?.name).toBe("Gains"));
  });

  it("lists reports under their folder toggles, which close and open", async () => {
    show();
    await screen.findByRole("button", { name: /My gains/ });
    expect(within(folderItem("Unfiled")).getByRole("button", { name: /My gains/ })).toBeTruthy();
    await fireEvent.click(button("Close Unfiled"));
    expect(screen.queryByRole("button", { name: /My gains/ })).toBeNull();
    await fireEvent.click(button("Open Unfiled"));
    expect(screen.getByRole("button", { name: /My gains/ })).toBeTruthy();
  });

  it("creates a folder and moves a report into it from the folder menu", async () => {
    show();
    await screen.findByRole("button", { name: /My gains/ });
    await newFolder("Titheable");
    await fireEvent.click(button(/My gains/));
    await fireEvent.click(button("Move to folder"));
    const menu = screen.getByRole("menu");
    expect(within(menu).getAllByRole("menuitem").map((b) => b.textContent)).toEqual(["Titheable", "Unfiled"]);
    expect((within(menu).getByRole("menuitem", { name: "Unfiled" }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Titheable" }));
    await vi.waitFor(() => expect(within(folderItem("Titheable")).queryByRole("button", { name: /My gains/ })).toBeTruthy());
    expect(within(folderItem("Unfiled")).queryByRole("button", { name: /My gains/ })).toBeNull();
    expect(db.reports[0].folder).toBe(10);
  });

  it("renames a report or a folder, never Unfiled", async () => {
    show();
    await screen.findByRole("button", { name: /My gains/ });
    await fireEvent.click(button(/^Unfiled/));
    expect((button("Rename") as HTMLButtonElement).disabled).toBe(true);
    expect((button("Delete") as HTMLButtonElement).disabled).toBe(true);

    await fireEvent.click(button(/My gains/));
    await fireEvent.click(button("Rename"));
    await fireEvent.input(screen.getByLabelText("Report name"), { target: { value: "Gains" } });
    await fireEvent.click(button("OK"));
    await screen.findByRole("button", { name: /^Gains/ });

    await newFolder("Old");
    await fireEvent.click(button(/^Old/));
    await fireEvent.click(button("Rename"));
    const field = screen.getByLabelText("Folder name") as HTMLInputElement;
    expect(field.value).toBe("Old");
    await fireEvent.input(field, { target: { value: "Archive" } });
    await fireEvent.keyDown(field, { key: "Enter" });
    await screen.findByRole("button", { name: /^Archive \d+$/ });
  });

  it("deletes a report after asking, and a folder only when it is empty", async () => {
    show();
    await screen.findByRole("button", { name: /My gains/ });
    await newFolder("Spare");
    await fireEvent.click(button(/My gains/));
    await fireEvent.click(button("Move to folder"));
    await fireEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Spare" }));
    await vi.waitFor(() => expect(db.reports[0].folder).toBe(10));

    await fireEvent.click(button(/^Spare/));
    expect((button("Delete") as HTMLButtonElement).disabled).toBe(true);

    await fireEvent.click(button(/My gains/));
    await fireEvent.click(button("Delete"));
    expect(confirmState.message).toBe('Delete the saved report "My gains"?');
    confirmState.answer(true);
    await vi.waitFor(() => expect(screen.queryByRole("button", { name: /My gains/ })).toBeNull());

    await fireEvent.click(button(/^Spare/));
    await fireEvent.click(button("Delete"));
    confirmState.answer(true);
    await vi.waitFor(() => expect(screen.queryByRole("button", { name: /^Spare/ })).toBeNull());
    expect(db.folders.map((f) => f.name)).toEqual(["Unfiled"]);
  });
});
