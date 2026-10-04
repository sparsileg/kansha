import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const openAcct = vi.hoisted(() => vi.fn());
const ins = vi.hoisted(() => ({
  list: vi.fn(),
  create: vi.fn(),
  update: vi.fn(),
  move: vi.fn(),
  remove: vi.fn(),
}));

vi.mock("../lib/api", async (orig) => {
  const real = await orig<typeof import("../lib/api")>();
  return {
    ...real,
    commands: {
      appVersion: () => Promise.resolve("0.1.0"),
      insightList: () => ins.list(),
      insightCreate: (name: string, cards: string[]) => ins.create(name, cards),
      insightUpdate: (id: number, name: string, cards: string[]) => ins.update(id, name, cards),
      insightMove: (id: number, delta: number) => ins.move(id, delta),
      insightDelete: (id: number) => ins.remove(id),
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
vi.mock("../lib/shell/nav", async (orig) => {
  const real = await orig<typeof import("../lib/shell/nav")>();
  return { ...real, openAccount: (id: number) => openAcct(id) };
});

import Dashboard from "./Insights.svelte";
import { CARDS } from "../lib/dashboard/cards";
import { bookSettings } from "../lib/state/booksettings.svelte";
import { confirmState } from "../lib/state/confirm.svelte";
import { listsState } from "../lib/state/lists.svelte";
import { viewState } from "../lib/state/view.svelte";
import type { Insight } from "../lib/types/bindings";

const ALL = CARDS.map((c) => c.id);
const status: Insight = { id: 1, name: "Status", cards: ALL };
const spending: Insight = { id: 2, name: "Spending", cards: ["attention", "net_worth"] };

beforeEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.restoreAllMocks();
  listsState.accounts = [{ id: 1, name: "Checking" }] as never;
  listsState.insights = [status];
  bookSettings.reset();
  viewState.reset();
});

const shownCards = (container: HTMLElement) =>
  [...container.querySelectorAll("[data-card]")].map((e) => e.getAttribute("data-card"));
const tabs = () => screen.getAllByRole("tab").map((t) => [t.textContent?.trim(), t.getAttribute("aria-selected")]);

async function gear(item: string) {
  await fireEvent.click(screen.getByRole("button", { name: "Insight options" }));
  await fireEvent.click(screen.getByRole("menuitem", { name: item }));
}

describe("Insights: the Status insight (DSH-010 … DSH-030)", () => {
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
    expect(screen.getByRole("article", { name: "Due in the next 14 days" }).classList.contains("double")).toBe(true);
  });

  it("the title sits in a shaded band inside one outlined sheet holding the cards", async () => {
    const { container } = render(Dashboard);
    await screen.findByText("10,019,506.27");
    const sheet = container.querySelector(".view-sheet") as HTMLElement;
    const title = sheet.querySelector(":scope > .view-title") as HTMLElement;
    expect(title.contains(screen.getByRole("heading", { level: 1, name: "Insights" }))).toBe(true);
    expect(title.contains(screen.getByRole("tab", { name: "Status" }))).toBe(true);
    const body = sheet.querySelector(":scope > .view-body") as HTMLElement;
    expect(body.querySelectorAll("[data-card]").length).toBe(CARDS.length);
  });

});

describe("Insights: tabs and the gear (INS-010 … INS-040)", () => {
  it("shows one tab per insight; the first opens unless one is named", async () => {
    listsState.insights = [status, spending];
    const { container } = render(Dashboard);
    await screen.findByText("10,019,506.27");
    expect(tabs()).toEqual([
      ["Status", "true"],
      ["Spending", "false"],
    ]);
    await fireEvent.click(screen.getByRole("tab", { name: "Spending" }));
    expect(viewState.params.insight).toBe(2);
    await waitFor(() => expect(shownCards(container)).toEqual(["attention", "net_worth"]));
    expect(tabs()[1]).toEqual(["Spending", "true"]);
  });

  it("the same card can be on two insights", async () => {
    listsState.insights = [status, spending];
    viewState.navigate("dashboard", { insight: 2 });
    const { container } = render(Dashboard);
    await screen.findByText("10,019,506.27");
    expect(shownCards(container)).toEqual(["attention", "net_worth"]);
    await fireEvent.click(screen.getByRole("tab", { name: "Status" }));
    await waitFor(() => expect(shownCards(container)).toEqual(ALL));
  });

  it("Create shows a new tab with the modal; Cancel drops it and makes nothing", async () => {
    render(Dashboard);
    await screen.findByText("10,019,506.27");
    await gear("Create new insight…");
    expect(tabs()).toEqual([
      ["Status", "false"],
      ["New insight", "true"],
    ]);
    const dlg = screen.getByRole("dialog", { name: "New insight" });
    await fireEvent.click(within(dlg).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(tabs()).toEqual([["Status", "true"]]);
    expect(ins.create).not.toHaveBeenCalled();
  });

  it("Create's Save makes the insight with its name and cards and opens its tab", async () => {
    const made: Insight = { id: 7, name: "Plans", cards: ["upcoming", "net_worth"] };
    ins.create.mockReturnValue(ok(made));
    ins.list.mockReturnValue(ok([status, made]));
    render(Dashboard);
    await screen.findByText("10,019,506.27");
    await gear("Create new insight…");
    const dlg = screen.getByRole("dialog", { name: "New insight" });
    const save = within(dlg).getByRole("button", { name: "Save" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true); // a name is needed
    await fireEvent.input(within(dlg).getByRole("textbox", { name: "Name" }), { target: { value: " Plans " } });
    const avail = within(dlg).getByRole("listbox", { name: "Available cards" });
    for (const id of ["net_worth", "upcoming"]) {
      await fireEvent.change(avail, { target: { value: id } });
      await fireEvent.click(within(dlg).getByRole("button", { name: "Add ›" }));
    }
    // Due soon was added last and is selected: move it first.
    await fireEvent.click(within(dlg).getByRole("button", { name: "Move up (earlier)" }));
    await fireEvent.click(save);
    await waitFor(() => expect(ins.create).toHaveBeenCalledWith("Plans", ["upcoming", "net_worth"]));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(viewState.params.insight).toBe(7);
    expect(tabs()).toEqual([
      ["Status", "false"],
      ["Plans", "true"],
    ]);
  });

  it("Customize renames the insight and sets its cards", async () => {
    ins.update.mockImplementation((id: number, name: string, cards: string[]) => ok({ id, name, cards }));
    ins.list.mockReturnValue(ok([{ id: 1, name: "Overview", cards: ["this_month"] }]));
    render(Dashboard);
    await screen.findByText("10,019,506.27");
    await gear("Customize…");
    const dlg = screen.getByRole("dialog", { name: "Customize insight" });
    const name = within(dlg).getByRole("textbox", { name: "Name" }) as HTMLInputElement;
    expect(name.value).toBe("Status");
    await fireEvent.input(name, { target: { value: "Overview" } });
    const shown = within(dlg).getByRole("listbox", { name: "On this insight (in order)" });
    for (const id of ["net_worth", "net_worth_trend", "upcoming", "attention"]) {
      await fireEvent.change(shown, { target: { value: id } });
      await fireEvent.click(within(dlg).getByRole("button", { name: "‹ Remove" }));
    }
    await fireEvent.click(within(dlg).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(ins.update).toHaveBeenCalledWith(1, "Overview", ["this_month"]));
    await waitFor(() => expect(screen.getByRole("tab", { name: "Overview" })).toBeTruthy());
  });

  it("a refused save stays open with the reason", async () => {
    ins.create.mockReturnValue(
      Promise.resolve({ status: "error", error: { kind: "invalid", message: "an insight named \"Status\" already exists" } }),
    );
    render(Dashboard);
    await screen.findByText("10,019,506.27");
    await gear("Create new insight…");
    const dlg = screen.getByRole("dialog", { name: "New insight" });
    await fireEvent.input(within(dlg).getByRole("textbox", { name: "Name" }), { target: { value: "Status" } });
    await fireEvent.click(within(dlg).getByRole("button", { name: "Save" }));
    expect((await within(dlg).findByRole("alert")).textContent).toMatch(/already exists/);
    expect(screen.getByRole("dialog", { name: "New insight" })).toBeTruthy();
  });

  it("moves and deletes are greyed out where they cannot apply", async () => {
    render(Dashboard);
    await screen.findByText("10,019,506.27");
    await fireEvent.click(screen.getByRole("button", { name: "Insight options" }));
    const off = (n: string) => (screen.getByRole("menuitem", { name: n }) as HTMLButtonElement).disabled;
    expect([off("Move left"), off("Move right"), off("Delete insight…")]).toEqual([true, true, true]);
    expect(off("Customize…")).toBe(false);
  });

  it("Move right reorders the tabs", async () => {
    listsState.insights = [status, spending];
    ins.move.mockReturnValue(ok([spending, status]));
    render(Dashboard);
    await screen.findByText("10,019,506.27");
    await gear("Move right");
    await waitFor(() => expect(ins.move).toHaveBeenCalledWith(1, 1));
    await waitFor(() => expect(tabs().map((t) => t[0])).toEqual(["Spending", "Status"]));
  });

  it("Delete asks first, then removes the insight and shows the first tab", async () => {
    listsState.insights = [status, spending];
    viewState.navigate("dashboard", { insight: 2 });
    const ask = vi.spyOn(confirmState, "ask").mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    ins.remove.mockReturnValue(ok(null));
    ins.list.mockReturnValue(ok([status]));
    render(Dashboard);
    await screen.findByText("10,019,506.27");
    await gear("Delete insight…");
    await waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    expect(ins.remove).not.toHaveBeenCalled();
    await gear("Delete insight…");
    await waitFor(() => expect(ins.remove).toHaveBeenCalledWith(2));
    await waitFor(() => expect(tabs()).toEqual([["Status", "true"]]));
    expect(viewState.params.insight).toBeUndefined();
  });
});
