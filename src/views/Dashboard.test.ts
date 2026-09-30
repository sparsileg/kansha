import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const openAcct = vi.hoisted(() => vi.fn());
const settingsSet = vi.hoisted(() => vi.fn());

vi.mock("../lib/api", async (orig) => {
  const real = await orig<typeof import("../lib/api")>();
  return {
    ...real,
    commands: {
      appVersion: () => Promise.resolve("0.1.0"),
      settingsSet: (s: unknown) => settingsSet(s),
      dashboard: () =>
        ok({
          today: "2026-09-27",
          net_worth: "10019506.27",
          cash: "121251.89",
          investments: "9176614.40",
          other_assets: "725100.00",
          liabilities: "3460.02",
          month_from: "2026-09-01",
          income: "5000.00",
          expenses: "1234.56",
          net: "3765.44",
          trend: { dates: ["2026-08-31", "2026-09-27"], labels: [], series: [{ name: "Net Worth", style: "line", values: ["1.00", "2.00"], pos: [5000, 10000] }], ticks: [{ label: "0", pos: 0 }, { label: "2", pos: 10000 }], zero: 0, x_unit: "month" },
          upcoming: [{ schedule: 1, nominal: "2026-09-20", date: "2026-09-20", amount: "-50.00", status: "pending", account: 1, payee: null, estimated: false, mode: "remind", overridden: false, txn: null, needs_review: false, overdue: true, actionable: true }],
          upcoming_days: 14,
          warnings: [
            { kind: "unreconciled", message: "Checking has uncleared transactions more than 60 days old.", account: 1 },
            { kind: "backup", message: "The backup folder is missing, so backups go to Downloads. Choose a folder in Settings.", account: null },
          ],
          backup: { last_at: "2026-09-26T22:00:00Z", last_path: "/x.zip", last_issues: 0, last_verified_at: null, folder_missing: true },
        }),
    },
  };
});
vi.mock("../lib/shell/nav", () => ({ openAccount: (id: number) => openAcct(id) }));

import Dashboard from "./Dashboard.svelte";
import { CARDS } from "../lib/dashboard/cards";
import { bookSettings } from "../lib/state/booksettings.svelte";
import { listsState } from "../lib/state/lists.svelte";

beforeEach(() => {
  cleanup();
  vi.clearAllMocks();
  listsState.accounts = [{ id: 1, name: "Checking" }] as never;
  bookSettings.reset();
  settingsSet.mockImplementation((s: unknown) => ok(s));
});

describe("Dashboard", () => {
  it("shows net worth, its parts, this month, what is due, and warnings", async () => {
    render(Dashboard);
    expect(await screen.findByText("10,019,506.27")).toBeTruthy();
    for (const t of ["121,251.89", "9,176,614.40", "725,100.00", "−3,460.02", "5,000.00", "1,234.56", "3,765.44"]) {
      expect(screen.getByText(t)).toBeTruthy();
    }
    expect(screen.getByText("Overdue")).toBeTruthy();
    expect(screen.getByRole("img", { name: /Net Worth/ })).toBeTruthy();
    // Backup status and the missing-folder warning (DSH-030).
    expect(screen.getByText(/backup folder is missing/)).toBeTruthy();
    expect(screen.getByText(/Last backup: 2026-09-26T22:00:00Z\. Last full verification:\s+never\./)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: /uncleared/ }));
    expect(openAcct).toHaveBeenCalledWith(1);
  });

  it("shows each card, in order, named by its ID and heading", async () => {
    const { container } = render(Dashboard);
    await screen.findByText("10,019,506.27");
    const ids = [...container.querySelectorAll("[data-card]")].map((e) => e.getAttribute("data-card"));
    expect(ids).toEqual(CARDS.map((c) => c.id));
    expect(screen.getByRole("article", { name: "Net worth" })).toBeTruthy();
    expect(screen.getByRole("article", { name: "Due in the next 14 days" })).toBeTruthy();
  });

  it("the title sits in a shaded band inside one outlined sheet holding the cards", async () => {
    const { container } = render(Dashboard);
    await screen.findByText("10,019,506.27");
    const sheet = container.querySelector(".view-sheet") as HTMLElement;
    const title = sheet.querySelector(":scope > .view-title") as HTMLElement;
    expect(title.contains(screen.getByRole("heading", { level: 1, name: "Dashboard" }))).toBe(true);
    const body = sheet.querySelector(":scope > .view-body") as HTMLElement;
    expect(body.querySelectorAll("[data-card]").length).toBe(CARDS.length);
  });

  it("shows the cards the book's layout chooses, in its order (DSH-040)", async () => {
    bookSettings.value.dashboard_cards = JSON.stringify({ order: ["attention", "net_worth"], hidden: ["this_month", "upcoming"] });
    const { container } = render(Dashboard);
    await screen.findByText("10,019,506.27");
    const ids = [...container.querySelectorAll("[data-card]")].map((e) => e.getAttribute("data-card"));
    expect(ids).toEqual(["attention", "net_worth", "net_worth_trend"]);
  });

  it("the gear's Customize hides and moves cards and stores the layout", async () => {
    const { container } = render(Dashboard);
    await screen.findByText("10,019,506.27");
    await fireEvent.click(screen.getByRole("button", { name: "Dashboard options" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Customize…" }));
    const dlg = screen.getByRole("dialog", { name: "Customize dashboard" });
    await fireEvent.click(within(dlg).getByRole("checkbox", { name: "This month" }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "Select Needs attention" }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "Move Up" }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(settingsSet).toHaveBeenCalled());
    const saved = settingsSet.mock.calls[0][0] as { dashboard_cards: string };
    expect(JSON.parse(saved.dashboard_cards)).toEqual({
      order: ["net_worth", "this_month", "net_worth_trend", "attention", "upcoming"],
      hidden: ["this_month"],
    });
    await waitFor(() => {
      const ids = [...container.querySelectorAll("[data-card]")].map((e) => e.getAttribute("data-card"));
      expect(ids).toEqual(["net_worth", "net_worth_trend", "attention", "upcoming"]);
    });
  });

  it("Reset stores nothing, so the default applies", async () => {
    bookSettings.value.dashboard_cards = JSON.stringify({ order: ["attention"], hidden: ["upcoming"] });
    render(Dashboard);
    await screen.findByText("10,019,506.27");
    await fireEvent.click(screen.getByRole("button", { name: "Dashboard options" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Customize…" }));
    const dlg = screen.getByRole("dialog", { name: "Customize dashboard" });
    await fireEvent.click(within(dlg).getByRole("button", { name: "Reset" }));
    await fireEvent.click(within(dlg).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(settingsSet).toHaveBeenCalled());
    expect((settingsSet.mock.calls[0][0] as { dashboard_cards: string | null }).dashboard_cards).toBeNull();
  });
});
