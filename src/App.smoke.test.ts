import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/svelte";

const book = vi.hoisted(() => ({
  state: "locked" as string,
  open: true,
  settings: null as unknown,
  unlocks: [] as string[],
  setups: [] as unknown[][],
  picks: 0,
}));

vi.mock("./lib/api", async (orig) => {
  const real = await orig<typeof import("./lib/api")>();
  const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
  // Not imported from booksettings: it imports this mocked module.
  const DEFAULT_SETTINGS = {
    date_format: "mdy",
    week_start: "sunday",
    startup: "dashboard",
    integrity_at_startup: false,
    nav_items: null,
    account_panel_open: true,
    account_panel_side: "left",
    account_panel_width: 0,
    invest_views: null,
    stale_price_days: 7,
    default_lot_method: "fifo",
    price_download: false,
    upcoming_days: 14,
    backup_folder: null,
    backup_keep_last: 10,
    backup_keep_months: 12,
    backup_timeout_minutes: 5,
    gray_reconciled: true,
    recall_payees: true,
    capitalize_names: false,
    auto_memorize_payees: true,
    purge_payees_months: 0,
    warn_out_of_date: true,
    warn_check_reuse: true,
    confirm_save_change: false,
  };
  const acct = {
    id: 1,
    name: "Savings",
    account_type: "savings",
    group: "banking",
    status: "open",
    show_in_list: true,
    sort_order: 0,
    investment: null,
  };
  return {
    ...real,
    commands: {
      appVersion: () => Promise.resolve("x"),
      bookStatus: () =>
        ok({
          state: book.state,
          open: book.open,
          name: "kansha",
          db_path: "/data/kansha.db",
          folder: "/data",
          downloads: "/home/u/Downloads",
        }),
      bookRecent: () =>
        Promise.resolve([
          {
            name: "kansha",
            path: "/data/kansha.db",
            exists: true,
            current: true,
          },
        ]),
      pickBookFile: () => {
        book.picks += 1;
        return Promise.resolve(null);
      },
      bookSetup: (...args: unknown[]) => {
        book.setups.push(args);
        book.open = true;
        return ok(null);
      },
      bookUnlock: (p: string) => {
        book.unlocks.push(p);
        if (p !== "right") {
          return Promise.resolve({
            status: "error" as const,
            error: {
              kind: "wrong_passphrase" as const,
              message: "the passphrase is not correct",
            },
          });
        }
        book.open = true;
        return ok(null);
      },
      payeesForgetStale: () => ok(0),
      settingsGet: () => ok(book.settings ?? DEFAULT_SETTINGS),
      settingsSet: (s: unknown) => {
        book.settings = s;
        return ok(s);
      },
      appearanceSet: () => ok(null),
      backupInfo: () =>
        ok({
          status: {
            last_at: null,
            last_path: null,
            last_issues: 0,
            last_verified_at: null,
            folder_missing: false,
          },
          folder: "/home/u/Downloads",
          folder_missing_now: false,
        }),
      today: () => ok("2026-09-24"),
      accountList: () => ok([acct]),
      accountBalances: () =>
        ok([{ account: 1, current: "10.00", ending: "10.00" }]),
      categoryList: () => ok([]),
      tagList: () => ok([]),
      payeeList: () => ok([]),
      taxLineList: () => ok([]),
      insightList: () => ok([]),
      dashboard: () =>
        Promise.resolve({
          status: "error" as const,
          error: { kind: "internal" as const, message: "not in this test" },
        }),
      registerQuery: () => ok({ rows: [], total: 0, today: "2026-09-24" }),
      registerSummary: () =>
        ok({
          current: "10.00",
          cleared: "0.00",
          ending: "10.00",
          available_credit: null,
        }),
      payeeSearch: () => ok([]),
      scheduleAutoEnter: () => ok({ entered: [], failed: [] }),
      scheduleList: () => ok([]),
      scheduleDueList: () => ok([]),
      scheduleReviewList: () => ok([]),
      calendarOccurrences: () => ok([]),
      calendarTransactions: () => ok([]),
      calendarProjection: () => ok([]),
    },
  };
});

import App from "./App.svelte";
import { registerState } from "./lib/state/register.svelte";
import { settingsState } from "./lib/state/settings.svelte";
import { viewState } from "./lib/state/view.svelte";
import { windowState } from "./lib/state/windows.svelte";

const account = () => screen.findByRole("button", { name: /Savings/ });

beforeEach(() => {
  book.open = true;
  book.state = "locked";
  book.settings = null;
  book.unlocks = [];
  settingsState.setAccountPanelOpen(true);
  settingsState.setStartup("dashboard");
  registerState.accountId = null;
  windowState.reset();
  viewState.reset();
});

describe("App shell", () => {
  it("shows the menu bar, the quick-jump bar, and the account list", async () => {
    render(App);
    await screen.findByRole("button", { name: "File" });
    for (const m of ["File", "Edit", "Tools", "Reports", "Help"]) {
      expect(screen.getByRole("button", { name: m })).toBeTruthy();
    }
    expect(screen.getByRole("button", { name: "Insights" })).toBeTruthy();
    expect(screen.getByRole("searchbox", { name: "Search" })).toHaveProperty(
      "disabled",
      false,
    );
    await account();
  });

  it("selecting an account shows its register and status line", async () => {
    render(App);
    await fireEvent.click(await account());
    await waitFor(() => expect(screen.getByRole("grid")).toBeTruthy());
    expect(screen.getByLabelText("Payment")).toBeTruthy();
    expect(screen.getByText("0 transactions")).toBeTruthy();
    // Current and Ending sit at the bottom right, not in the header.
    expect(screen.getByText("Ending")).toBeTruthy();
  });

  it("switches views from the menus and the quick-jump bar", async () => {
    render(App);
    await fireEvent.click(await account());
    await waitFor(() => expect(screen.getByRole("grid")).toBeTruthy());
    await fireEvent.click(screen.getByRole("button", { name: "Insights" }));
    await waitFor(() => expect(screen.queryByRole("grid")).toBeNull());
    await fireEvent.click(screen.getByRole("button", { name: "Tools" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Categories" }));
    await waitFor(() =>
      expect(
        screen.getByRole("tab", { name: "Categories", selected: true }),
      ).toBeTruthy(),
    );
    await fireEvent.click(screen.getByRole("tab", { name: "Tags" }));
    await waitFor(() =>
      expect(
        screen.getByRole("tab", { name: "Tags", selected: true }),
      ).toBeTruthy(),
    );
  });

  it("opens the reminders list and the calendar", async () => {
    render(App);
    await account();
    await fireEvent.click(screen.getByRole("button", { name: /^Reminders/ }));
    await waitFor(() =>
      expect(screen.getByText("No scheduled transactions yet.")).toBeTruthy(),
    );
    expect(screen.getByRole("heading", { name: "Reminders" })).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Calendar" }));
    await waitFor(() =>
      expect(screen.getByRole("grid", { name: "Month" })).toBeTruthy(),
    );
    expect(screen.getByText("September 2026")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Next month" }));
    await waitFor(() => expect(screen.getByText("October 2026")).toBeTruthy());
  });

  it("gets back to the current account from another view by clicking it in the list", async () => {
    render(App);
    await fireEvent.click(await account());
    await waitFor(() => expect(screen.getByRole("grid")).toBeTruthy());
    await fireEvent.click(screen.getByRole("button", { name: "Calendar" }));
    await waitFor(() =>
      expect(screen.getByRole("grid", { name: "Month" })).toBeTruthy(),
    );
    await fireEvent.click(await account());
    await waitFor(() => expect(screen.getByLabelText("Payment")).toBeTruthy());
  });

  it("shows the open book's name on the menu bar, its path on hover", async () => {
    render(App);
    await account();
    const bar = screen.getByRole("navigation", { name: "Menu bar" });
    const name = within(bar).getByText("kansha");
    expect(name.getAttribute("title")).toBe("/data/kansha.db");
  });

  it("opens Settings and the Accounts list from the menus", async () => {
    render(App);
    await account();
    await fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: /Settings/ }));
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    // Theme and font size are on the menu bar, not in Settings.
    expect(
      within(dialog).queryByRole("combobox", { name: "Theme" }),
    ).toBeNull();
    expect(
      within(dialog).queryByRole("combobox", { name: "Font size" }),
    ).toBeNull();
    await fireEvent.click(
      within(dialog).getAllByRole("button", { name: "Close" }).at(-1)!,
    );
    await fireEvent.click(screen.getByRole("button", { name: "Tools" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Accounts" }));
    expect(
      await screen.findByRole("heading", { name: "Accounts" }),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Edit Savings" })).toBeTruthy();
  });

  it("picks the theme, font, and font size from the right end of the menu bar", async () => {
    render(App);
    const bar = await screen.findByRole("navigation", { name: "Menu bar" });
    const theme = within(bar).getByRole("combobox", { name: "Theme" });
    const font = within(bar).getByRole("combobox", { name: "Font" });
    const size = within(bar).getByRole("combobox", { name: "Font size" });
    // Theme, font, size, in that order, all after the menus.
    const order = Array.from(bar.querySelectorAll("button, select"));
    expect(order.indexOf(theme)).toBe(order.length - 3);
    expect(order.indexOf(font)).toBe(order.length - 2);
    expect(order.indexOf(size)).toBe(order.length - 1);
    expect(
      within(font)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["System", "Arial", "Verdana", "Courier New"]);
    await fireEvent.change(theme, { target: { value: "classic" } });
    await fireEvent.change(font, { target: { value: "verdana" } });
    await fireEvent.change(size, { target: { value: "17" } });
    await waitFor(() =>
      expect(document.documentElement.dataset.theme).toBe("classic"),
    );
    expect(
      document.documentElement.style.getPropertyValue("--font-ui"),
    ).toMatch(/^Verdana,/);
    expect(document.documentElement.style.fontSize).toBe("17px");
  });

  it("starts on the home screen setting", async () => {
    settingsState.setStartup("calendar");
    render(App);
    await waitFor(() =>
      expect(screen.getByRole("grid", { name: "Month" })).toBeTruthy(),
    );
  });

  it("starts locked: a wrong passphrase is asked again; the right one opens the book", async () => {
    book.open = false;
    render(App);
    const field = await screen.findByLabelText("Backup passphrase");
    expect(screen.queryByRole("button", { name: "File" })).toBeNull();
    await fireEvent.input(field, { target: { value: "wrong" } });
    await fireEvent.click(screen.getByRole("button", { name: "Open" }));
    expect(
      await screen.findByText("the passphrase is not correct"),
    ).toBeTruthy();
    await fireEvent.input(screen.getByLabelText("Backup passphrase"), {
      target: { value: "right" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Open" }));
    await account();
    expect(book.unlocks).toEqual(["wrong", "right"]);
  });

  it("a locked book still offers Create a new book…, with an empty name to fill in", async () => {
    book.open = false;
    book.state = "locked";
    book.setups = [];
    render(App);
    await fireEvent.click(await screen.findByRole("button", { name: "Create a new book…" }));
    expect(screen.getByText(/Create a new, empty book/)).toBeTruthy();
    const create = screen.getByRole("button", { name: "Create book" }) as HTMLButtonElement;
    await fireEvent.input(screen.getByLabelText("Passphrase"), { target: { value: "p" } });
    await fireEvent.input(screen.getByLabelText("Again"), { target: { value: "p" } });
    expect(create.disabled).toBe(true); // no name yet
    await fireEvent.input(screen.getByLabelText("Name"), { target: { value: "fresh" } });
    expect(create.disabled).toBe(false);
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByLabelText("Backup passphrase")).toBeTruthy();
  });

  it("first run shows setup: folder, passphrase twice, and the lost-passphrase warning", async () => {
    book.open = false;
    book.state = "new";
    render(App);
    expect(await screen.findByText(/Create a new, empty book/)).toBeTruthy();
    expect(screen.getByText("/home/u/Downloads (default)")).toBeTruthy();
    expect(
      screen.getByText(/If it is lost, they cannot be recovered/),
    ).toBeTruthy();
    const create = screen.getByRole("button", {
      name: "Create book",
    }) as HTMLButtonElement;
    expect(create.disabled).toBe(true);
    await fireEvent.input(screen.getByLabelText("Passphrase"), {
      target: { value: "a" },
    });
    await fireEvent.input(screen.getByLabelText("Again"), {
      target: { value: "b" },
    });
    expect(screen.getByText("The passphrases do not match.")).toBeTruthy();
    expect(create.disabled).toBe(true);
  });

  it("first run names the new book; a bad name cannot be used", async () => {
    book.open = false;
    book.state = "new";
    book.setups = [];
    render(App);
    const name = (await screen.findByLabelText("Name")) as HTMLInputElement;
    expect(name.value).toBe("kansha");
    expect(screen.getByText("/data")).toBeTruthy();
    await fireEvent.input(screen.getByLabelText("Passphrase"), {
      target: { value: "p" },
    });
    await fireEvent.input(screen.getByLabelText("Again"), {
      target: { value: "p" },
    });
    const create = screen.getByRole("button", {
      name: "Create book",
    }) as HTMLButtonElement;
    await fireEvent.input(name, { target: { value: "bad name" } });
    expect(create.disabled).toBe(true);
    await fireEvent.input(name, { target: { value: "barton2026" } });
    expect(create.disabled).toBe(false);
    await fireEvent.click(create);
    expect(book.setups).toEqual([["p", null, "barton2026", null]]);
  });

  it("first run can open an existing book instead", async () => {
    book.open = false;
    book.state = "new";
    book.picks = 0;
    render(App);
    await fireEvent.click(
      await screen.findByRole("button", { name: "Open an existing book…" }),
    );
    expect(book.picks).toBe(1);
  });

  it("File offers New, Open, recent books, and Rename Book", async () => {
    render(App);
    await account();
    await fireEvent.click(screen.getByRole("button", { name: "File" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: /New/ }));
    const dialog = await screen.findByRole("dialog", { name: "New book" });
    expect(
      within(dialog).getByRole("button", { name: "Create book" }),
    ).toBeTruthy();
    await fireEvent.click(
      within(dialog).getByRole("button", { name: "Cancel" }),
    );
    await fireEvent.click(screen.getByRole("button", { name: "File" }));
    await fireEvent.click(
      screen.getByRole("menuitem", { name: /Rename Book/ }),
    );
    const rename = await screen.findByRole("dialog", { name: "Rename book" });
    expect(
      (within(rename).getByLabelText("New name") as HTMLInputElement).value,
    ).toBe("kansha");
  });

  it("an unencrypted prototype book is offered for encryption", async () => {
    book.open = false;
    book.state = "unencrypted";
    render(App);
    expect(
      await screen.findByText(/Keep the existing book; it will be encrypted/),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Encrypt book" })).toBeTruthy();
  });
});
