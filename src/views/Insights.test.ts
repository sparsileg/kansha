import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const openAcct = vi.hoisted(() => vi.fn());
const trendCall = vi.hoisted(() => vi.fn());
const attentionCall = vi.hoisted(() => vi.fn());
const autoCall = vi.hoisted(() => vi.fn());
const sc = vi.hoisted(() => ({
  create: vi.fn(),
  update: vi.fn(),
  remove: vi.fn(),
}));
const CHECKS = (bad: string[]) =>
  [
    ["integrity", "Database integrity"],
    ["backup", "Changes backed up"],
    ["verification", "Last backup verified"],
    ["backup_folder", "Backup folder"],
    ["prices", "Security prices"],
    ["overdue", "Overdue reminders"],
    ["uncleared", "Old uncleared transactions"],
    ["reconcile", "Accounts reconciled"],
    ["uncategorized", "Uncategorized transactions"],
    ["investment_cash", "Investment cash"],
  ].map(([kind, label]) => ({
    kind,
    label,
    ok: !bad.includes(kind),
    detail: kind === "backup" ? "last backup 2 hours ago" : null,
  }));
const notice = (
  kind: string,
  message: string,
  remedy: string,
  items: { message: string; account: number | null }[] = [],
  more = 0,
) => ({ kind, message, items, more, remedy });
const PROBLEMS = {
  checked_at: "2026-09-27T08:00:00Z",
  as_of: "4:00 AM",
  checks: CHECKS(["integrity", "backup_folder", "uncleared"]),
  notices: [
    notice(
      "integrity",
      "The database has 2 integrity problems",
      "Show the details and fix each one.",
    ),
    notice(
      "backup_folder",
      "The backup folder /usb is not there, so backups go to Downloads",
      "Choose a folder in Edit > Settings.",
    ),
    notice(
      "uncleared",
      "6 accounts have uncleared transactions more than 60 days old",
      "Open each account and clear or reconcile them.",
      [
        { message: "Checking", account: 1 },
        { message: "Visa", account: 2 },
      ],
      4,
    ),
  ],
};
const CLEAR = {
  checked_at: "2026-09-27T08:00:00Z",
  as_of: "4:00 AM",
  checks: CHECKS([]),
  notices: [],
};
const attn = vi.hoisted(() => ({ value: null as unknown }));
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
      insightUpdate: (id: number, name: string, cards: string[]) =>
        ins.update(id, name, cards),
      insightMove: (id: number, delta: number) => ins.move(id, delta),
      insightDelete: (id: number) => ins.remove(id),
      netWorthTrend: (years: number, fitted: boolean) => {
        trendCall(years, fitted);
        return ok({
          dates: ["2026-08-31", "2026-09-27"],
          labels: [],
          series: [
            {
              name: "Net Worth",
              style: "line",
              values: ["1.00", "2.00"],
              pos: [5000, 10000],
            },
          ],
          ticks: [
            { label: "0", pos: 0 },
            { label: "2", pos: 10000 },
          ],
          zero: 0,
          x_unit: "month",
        });
      },
      settingsSet: (s: unknown) => ok(s),
      spendingCardData: (id: number) =>
        ok(
          autoCall(id) ?? {
            rows: [],
            total: {
              category: null,
              label: "Total",
              ytd: "0.00",
              mtd: "0.00",
              monthly_avg: "0.00",
              scheduled: "0.00",
            },
            chosen: 0,
          },
        ),
      spendingCardList: () => ok(listsState.spendingCards),
      spendingCardCreate: (name: string) => sc.create(name),
      spendingCardUpdate: (
        id: number,
        name: string,
        accounts: number[] | null,
        categories: number[],
      ) => sc.update(id, name, accounts, categories),
      spendingCardDelete: (id: number) => sc.remove(id),
      attention: () => {
        attentionCall();
        return ok(attn.value ?? PROBLEMS);
      },
      cardData: () =>
        ok({
          today: "2026-09-27",
          net_worth: "10019506.27",
          years: [2024, 2025, 2026],
          groups: [
            {
              group: "banking",
              label: "Banking",
              balances: ["100000.00", "110000.00", "121251.89"],
            },
            {
              group: "credit",
              label: "Credit",
              balances: ["-1000.00", "-2000.00", "-3460.02"],
            },
            {
              group: "investments",
              label: "Investments",
              balances: ["8000000.00", "8500000.00", "9176614.40"],
            },
            {
              group: "assets",
              label: "Assets",
              balances: ["700000.00", "710000.00", "725100.00"],
            },
          ],
          net_worths: ["8799000.00", "9318000.00", "10019506.27"],
          month_from: "2026-09-01",
          income: "5000.00",
          expenses: "1234.56",
          net: "3765.44",
          upcoming: [
            {
              schedule: 1,
              nominal: "2026-09-20",
              date: "2026-09-20",
              amount: "-50.00",
              status: "pending",
              account: 1,
              payee: null,
              estimated: false,
              mode: "remind",
              overridden: false,
              txn: null,
              needs_review: false,
              overdue: true,
              actionable: true,
            },
          ],
          upcoming_days: 14,
        }),
    },
  };
});
vi.mock("../lib/shell/nav", async (orig) => {
  const real = await orig<typeof import("../lib/shell/nav")>();
  return { ...real, openAccount: (id: number) => openAcct(id) };
});

import Insights from "./Insights.svelte";
import { CARDS } from "../lib/insights/cards";
import { bookSettings } from "../lib/state/booksettings.svelte";
import { attentionState } from "../lib/state/attention.svelte";
import { confirmState } from "../lib/state/confirm.svelte";
import { dialogState } from "../lib/state/dialogs.svelte";
import { listsState } from "../lib/state/lists.svelte";
import { viewState } from "../lib/state/view.svelte";
import type { Insight, SpendingCard } from "../lib/types/bindings";

const ALL = CARDS.map((c) => c.id);
const status: Insight = { id: 1, name: "Status", cards: ALL };
const spending: Insight = {
  id: 2,
  name: "Spending",
  cards: ["attention", "net_worth"],
};

beforeEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.restoreAllMocks();
  listsState.accounts = [
    {
      id: 1,
      name: "Checking",
      group: "banking",
      status: "open",
      show_in_list: true,
    },
    {
      id: 2,
      name: "Old Visa",
      group: "credit",
      status: "closed",
      show_in_list: true,
    },
  ] as never;
  listsState.insights = [status];
  listsState.spendingCards = [];
  attn.value = null;
  bookSettings.reset();
  viewState.reset();
});

const shownCards = (container: HTMLElement) =>
  [...container.querySelectorAll("[data-card]")].map((e) =>
    e.getAttribute("data-card"),
  );
const tabs = () =>
  screen
    .getAllByRole("tab")
    .map((t) => [t.textContent?.trim(), t.getAttribute("aria-selected")]);

async function gear(item: string) {
  await fireEvent.click(
    screen.getByRole("button", { name: "Insight options" }),
  );
  await fireEvent.click(screen.getByRole("menuitem", { name: item }));
}

describe("Insights: the Status insight (CARD-010 … CARD-040)", () => {
  it("shows net worth, its parts, this month, what is due, and what needs attention", async () => {
    render(Insights);
    expect(await screen.findAllByText("10,019,506.27")).toBeTruthy();
    for (const t of [
      "121,251.89",
      "9,176,614.40",
      "725,100.00",
      "-3,460.02",
      "5,000.00",
      "1,234.56",
      "3,765.44",
    ]) {
      expect(screen.getByText(t)).toBeTruthy();
    }
    expect(screen.getByText("Overdue")).toBeTruthy();
    expect(screen.getByRole("img", { name: /Net Worth/ })).toBeTruthy();
    expect(
      await screen.findByText(/uncleared transactions more than 60 days old/),
    ).toBeTruthy();
  });

  it("shows each card, in order, named by its ID and heading", async () => {
    const { container } = render(Insights);
    await screen.findAllByText("10,019,506.27");
    const ids = [...container.querySelectorAll("[data-card]")].map((e) =>
      e.getAttribute("data-card"),
    );
    expect(ids).toEqual(CARDS.map((c) => c.id));
    expect(screen.getByRole("article", { name: "Net worth" })).toBeTruthy();
    expect(
      screen
        .getByRole("article", { name: "Due in the next 14 days" })
        .classList.contains("double"),
    ).toBe(true);
  });

  it("Net worth lists each group at the end of the last two years and today, then net worth (CARD-010)", async () => {
    render(Insights);
    const card = await screen.findByRole("article", { name: "Net worth" });
    await within(card).findAllByText("10,019,506.27");
    expect(card.classList.contains("double")).toBe(true);
    const rows = within(card)
      .getAllByRole("row")
      .map((r) =>
        [...r.querySelectorAll("th,td")].map((c) => c.textContent?.trim()),
      );
    expect(rows).toEqual([
      ["Group", "2024", "2025", "2026"],
      ["Banking", "100,000.00", "110,000.00", "121,251.89"],
      ["Credit", "-1,000.00", "-2,000.00", "-3,460.02"],
      ["Investments", "8,000,000.00", "8,500,000.00", "9,176,614.40"],
      ["Assets", "700,000.00", "710,000.00", "725,100.00"],
      ["Net worth", "8,799,000.00", "9,318,000.00", "10,019,506.27"],
    ]);
    expect(
      within(card)
        .getByRole("columnheader", { name: "2025" })
        .getAttribute("title"),
    ).toBe("December 31, 2025");
    expect(
      within(card)
        .getByRole("columnheader", { name: "2026" })
        .getAttribute("title"),
    ).toBe("Today");
  });

  it("the title sits in a shaded band inside one outlined sheet holding the cards", async () => {
    const { container } = render(Insights);
    await screen.findAllByText("10,019,506.27");
    const sheet = container.querySelector(".view-sheet") as HTMLElement;
    const title = sheet.querySelector(":scope > .view-title") as HTMLElement;
    expect(
      title.contains(
        screen.getByRole("heading", { level: 1, name: "Insights" }),
      ),
    ).toBe(true);
    expect(title.contains(screen.getByRole("tab", { name: "Status" }))).toBe(
      true,
    );
    const body = sheet.querySelector(":scope > .view-body") as HTMLElement;
    expect(body.querySelectorAll("[data-card]").length).toBe(CARDS.length);
  });
});

describe("Needs attention: problems only, each with what to do (CARD-030)", () => {
  const card = async () => {
    render(Insights);
    await screen.findByText(/found/);
    return screen.getByRole("article", { name: "Needs attention" });
  };
  const item = (kind: string) =>
    document.querySelector(`[data-notice="${kind}"]`) as HTMLElement;
  const text = (e: Element) => e.textContent?.replace(/\s+/g, " ").trim();

  it("lists each problem with its items and remedy, marked by symbol and words", async () => {
    const c = await card();
    expect(text(c.querySelector(".verdict")!)).toBe(
      "⚠ As of 4:00 AM: 3 problems found",
    );
    expect([...c.querySelectorAll("[data-notice]")].map(text)).toEqual([
      "The database has 2 integrity problems. Show the details and fix each one. Show details",
      "The backup folder /usb is not there, so backups go to Downloads. Choose a folder in Edit > Settings. Settings",
      "6 accounts have uncleared transactions more than 60 days old: Checking, Visa, and 4 more. Open each account and clear or reconcile them.",
    ]);
    expect(c.querySelector("[data-check]")).toBeNull();
  });

  it("links each item to where it is fixed", async () => {
    await card();
    await fireEvent.click(
      within(item("uncleared")).getByRole("button", { name: "Visa" }),
    );
    expect(openAcct).toHaveBeenCalledWith(2);
    await fireEvent.click(
      within(item("integrity")).getByRole("button", { name: "Show details" }),
    );
    expect(dialogState.integrity).toBe(true);
    expect(dialogState.integrityReport).toBe(null);
    await fireEvent.click(
      within(item("backup_folder")).getByRole("button", { name: "Settings" }),
    );
    expect(dialogState.settings).toBe(true);
  });

  it("says no problems were found when every check passes; Show checks lists them", async () => {
    attn.value = CLEAR;
    render(Insights);
    await screen.findByText(/No problems found/);
    const c = screen.getByRole("article", { name: "Needs attention" });
    expect(text(c.querySelector(".verdict")!)).toBe(
      "✓ As of 4:00 AM: No problems found. 10 checks passed.",
    );
    expect(c.querySelector("[data-notice]")).toBeNull();
    await fireEvent.click(
      within(c).getByRole("button", { name: "Show checks" }),
    );
    const checks = [...c.querySelectorAll("[data-check]")].map(text);
    expect(checks).toHaveLength(10);
    expect(checks[0]).toBe("✓ Database integrity: OK");
    expect(checks[1]).toBe("✓ Changes backed up: OK (last backup 2 hours ago)");
    await fireEvent.click(
      within(c).getByRole("button", { name: "Hide checks" }),
    );
    expect(c.querySelector("[data-check]")).toBeNull();
  });

  it("marks failed checks in the list by symbol and words", async () => {
    const c = await card();
    await fireEvent.click(
      within(c).getByRole("button", { name: "Show checks" }),
    );
    expect(text(c.querySelector('[data-check="integrity"]')!)).toBe(
      "⚠ Database integrity: Problem",
    );
    expect(text(c.querySelector('[data-check="prices"]')!)).toBe(
      "✓ Security prices: OK",
    );
  });

  it("loads only while shown, and again after a backup or a check", async () => {
    listsState.insights = [{ id: 3, name: "Money", cards: ["net_worth"] }];
    render(Insights);
    await screen.findAllByText("10,019,506.27");
    expect(attentionCall).not.toHaveBeenCalled();
    cleanup();
    listsState.insights = [status];
    await card();
    expect(attentionCall).toHaveBeenCalledTimes(1);
    attentionState.changed();
    await waitFor(() => expect(attentionCall).toHaveBeenCalledTimes(2));
  });
});

describe("Insights: tabs and the gear (INS-010 … INS-030)", () => {
  it("shows one tab per insight; the first opens unless one is named", async () => {
    listsState.insights = [status, spending];
    const { container } = render(Insights);
    await screen.findAllByText("10,019,506.27");
    expect(tabs()).toEqual([
      ["Status", "true"],
      ["Spending", "false"],
    ]);
    await fireEvent.click(screen.getByRole("tab", { name: "Spending" }));
    expect(viewState.params.insight).toBe(2);
    await waitFor(() =>
      expect(shownCards(container)).toEqual(["attention", "net_worth"]),
    );
    expect(tabs()[1]).toEqual(["Spending", "true"]);
  });

  it("the same card can be on two insights", async () => {
    listsState.insights = [status, spending];
    viewState.navigate("insights", { insight: 2 });
    const { container } = render(Insights);
    await screen.findAllByText("10,019,506.27");
    expect(shownCards(container)).toEqual(["attention", "net_worth"]);
    await fireEvent.click(screen.getByRole("tab", { name: "Status" }));
    await waitFor(() => expect(shownCards(container)).toEqual(ALL));
  });

  it("Create shows a new tab with the modal; Cancel drops it and makes nothing", async () => {
    render(Insights);
    await screen.findAllByText("10,019,506.27");
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
    const made: Insight = {
      id: 7,
      name: "Plans",
      cards: ["upcoming", "net_worth"],
    };
    ins.create.mockReturnValue(ok(made));
    ins.list.mockReturnValue(ok([status, made]));
    render(Insights);
    await screen.findAllByText("10,019,506.27");
    await gear("Create new insight…");
    const dlg = screen.getByRole("dialog", { name: "New insight" });
    const save = within(dlg).getByRole("button", {
      name: "Save",
    }) as HTMLButtonElement;
    expect(save.disabled).toBe(true); // a name is needed
    await fireEvent.input(within(dlg).getByRole("textbox", { name: "Name" }), {
      target: { value: " Plans " },
    });
    const avail = within(dlg).getByRole("listbox", { name: "Available cards" });
    for (const id of ["net_worth", "upcoming"]) {
      await fireEvent.change(avail, { target: { value: id } });
      await fireEvent.click(within(dlg).getByRole("button", { name: "Add ›" }));
    }
    // Due soon was added last and is selected: move it first.
    await fireEvent.click(
      within(dlg).getByRole("button", { name: "Move up (earlier)" }),
    );
    await fireEvent.click(save);
    await waitFor(() =>
      expect(ins.create).toHaveBeenCalledWith("Plans", [
        "upcoming",
        "net_worth",
      ]),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(viewState.params.insight).toBe(7);
    expect(tabs()).toEqual([
      ["Status", "false"],
      ["Plans", "true"],
    ]);
  });

  it("Customize renames the insight and sets its cards", async () => {
    ins.update.mockImplementation((id: number, name: string, cards: string[]) =>
      ok({ id, name, cards }),
    );
    ins.list.mockReturnValue(
      ok([{ id: 1, name: "Overview", cards: ["this_month"] }]),
    );
    render(Insights);
    await screen.findAllByText("10,019,506.27");
    await gear("Customize…");
    const dlg = screen.getByRole("dialog", { name: "Customize insight" });
    const name = within(dlg).getByRole("textbox", {
      name: "Name",
    }) as HTMLInputElement;
    expect(name.value).toBe("Status");
    await fireEvent.input(name, { target: { value: "Overview" } });
    const shown = within(dlg).getByRole("listbox", {
      name: "On this insight (in order)",
    });
    for (const id of [
      "net_worth",
      "net_worth_trend",
      "upcoming",
      "attention",
    ]) {
      await fireEvent.change(shown, { target: { value: id } });
      await fireEvent.click(
        within(dlg).getByRole("button", { name: "‹ Remove" }),
      );
    }
    await fireEvent.click(within(dlg).getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(ins.update).toHaveBeenCalledWith(1, "Overview", ["this_month"]),
    );
    await waitFor(() =>
      expect(screen.getByRole("tab", { name: "Overview" })).toBeTruthy(),
    );
  });

  it("a refused save stays open with the reason", async () => {
    ins.create.mockReturnValue(
      Promise.resolve({
        status: "error",
        error: {
          kind: "invalid",
          message: 'an insight named "Status" already exists',
        },
      }),
    );
    render(Insights);
    await screen.findAllByText("10,019,506.27");
    await gear("Create new insight…");
    const dlg = screen.getByRole("dialog", { name: "New insight" });
    await fireEvent.input(within(dlg).getByRole("textbox", { name: "Name" }), {
      target: { value: "Status" },
    });
    await fireEvent.click(within(dlg).getByRole("button", { name: "Save" }));
    expect((await within(dlg).findByRole("alert")).textContent).toMatch(
      /already exists/,
    );
    expect(screen.getByRole("dialog", { name: "New insight" })).toBeTruthy();
  });

  it("moves and deletes are greyed out where they cannot apply", async () => {
    render(Insights);
    await screen.findAllByText("10,019,506.27");
    await fireEvent.click(
      screen.getByRole("button", { name: "Insight options" }),
    );
    const off = (n: string) =>
      (screen.getByRole("menuitem", { name: n }) as HTMLButtonElement).disabled;
    expect([
      off("Move left"),
      off("Move right"),
      off("Delete insight…"),
    ]).toEqual([true, true, true]);
    expect(off("Customize…")).toBe(false);
  });

  it("Move right reorders the tabs", async () => {
    listsState.insights = [status, spending];
    ins.move.mockReturnValue(ok([spending, status]));
    render(Insights);
    await screen.findAllByText("10,019,506.27");
    await gear("Move right");
    await waitFor(() => expect(ins.move).toHaveBeenCalledWith(1, 1));
    await waitFor(() =>
      expect(tabs().map((t) => t[0])).toEqual(["Spending", "Status"]),
    );
  });

  it("Delete asks first, then removes the insight and shows the first tab", async () => {
    listsState.insights = [status, spending];
    viewState.navigate("insights", { insight: 2 });
    const ask = vi
      .spyOn(confirmState, "ask")
      .mockResolvedValueOnce(false)
      .mockResolvedValueOnce(true);
    ins.remove.mockReturnValue(ok(null));
    ins.list.mockReturnValue(ok([status]));
    render(Insights);
    await screen.findAllByText("10,019,506.27");
    await gear("Delete insight…");
    await waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    expect(ins.remove).not.toHaveBeenCalled();
    await gear("Delete insight…");
    await waitFor(() => expect(ins.remove).toHaveBeenCalledWith(2));
    await waitFor(() => expect(tabs()).toEqual([["Status", "true"]]));
    expect(viewState.params.insight).toBeUndefined();
  });
});

describe("Insights: Net worth over time (CARD-050)", () => {
  it("shows a year from zero until the card's controls change it, and keeps the choice", async () => {
    render(Insights);
    const card = await screen.findByRole("article", {
      name: "Net worth, last 12 months",
    });
    await waitFor(() => expect(trendCall).toHaveBeenLastCalledWith(1, false));
    expect(within(card).getByRole("img", { name: /Net Worth/ })).toBeTruthy();
    const years = within(card).getByRole("combobox", {
      name: "Years",
    }) as HTMLSelectElement;
    expect([...years.options].map((o) => o.text)).toEqual([
      "1 year",
      "2 years",
      "5 years",
    ]);

    await fireEvent.change(years, { target: { value: "5" } });
    await waitFor(() => expect(trendCall).toHaveBeenLastCalledWith(5, false));
    expect(bookSettings.value.trend_years).toBe(5);
    expect(
      screen.getByRole("article", { name: "Net worth, last 5 years" }),
    ).toBeTruthy();

    await fireEvent.click(
      within(card).getByRole("checkbox", { name: "Fit graph to data" }),
    );
    await waitFor(() => expect(trendCall).toHaveBeenLastCalledWith(5, true));
    expect(bookSettings.value.trend_fitted).toBe(true);
  });

  it("is not loaded for an insight without the card", async () => {
    listsState.insights = [spending];
    render(Insights);
    await screen.findAllByText("10,019,506.27");
    expect(trendCall).not.toHaveBeenCalled();
  });
});

describe("Insights: spending cards (CARD-060)", () => {
  const row = (
    category: number | null,
    label: string,
    ytd: string,
    mtd: string,
    monthly_avg: string,
  ) => ({ category, label, ytd, mtd, monthly_avg, scheduled: "0.00" });
  const carCard = (): SpendingCard => ({
    id: 4,
    name: "Car costs",
    accounts: null,
    categories: [],
  });
  const auto = (): Insight => ({ id: 3, name: "Car", cards: ["spending:4"] });
  const shown = (insights: Insight[] = [auto()]) => {
    listsState.spendingCards = [carCard()];
    listsState.insights = insights;
    ins.list.mockImplementation(() => ok(listsState.insights));
  };

  it("is headed by its name, and says how to choose when nothing is chosen", async () => {
    shown();
    render(Insights);
    const card = await screen.findByRole("article", { name: "Car costs" });
    expect(card.classList.contains("double")).toBe(true);
    await within(card).findByText(/Choose accounts and categories/);
    expect(autoCall).toHaveBeenCalledWith(4);
  });

  it("says so when no chosen category has spending this year", async () => {
    autoCall.mockReturnValue({
      rows: [],
      total: row(null, "Total", "0.00", "0.00", "0.00"),
      chosen: 2,
    });
    shown();
    render(Insights);
    const card = await screen.findByRole("article", { name: "Car costs" });
    await within(card).findByText(
      "No spending this year in the chosen categories.",
    );
    expect(
      within(card).queryByText(/Choose accounts and categories/),
    ).toBeNull();
  });

  it("lists each chosen category with Year, Month, and monthly average, then the total, under a title saying scheduled count", async () => {
    autoCall.mockReturnValue({
      rows: [
        row(7, "Car:BlueForester:Gas", "1711.19", "0.00", "171.11"),
        row(8, "Car:BlueForester:Insurance", "524.50", "0.00", "52.45"),
      ],
      total: row(null, "Total", "2235.69", "0.00", "223.57"),
      chosen: 2,
    });
    shown();
    render(Insights);
    const card = await screen.findByRole("article", { name: "Car costs" });
    await within(card).findByText("1,711.19");
    const rows = within(card)
      .getAllByRole("row")
      .map((r) =>
        [...r.querySelectorAll("th,td")].map((c) => c.textContent?.trim()),
      );
    expect(rows).toEqual([
      ["Category", "Year", "Month", "Monthly Avg"],
      ["Car:BlueForester:Gas", "1,711.19", "0.00", "171.11"],
      ["Car:BlueForester:Insurance", "524.50", "0.00", "52.45"],
      ["Total", "2,235.69", "0.00", "223.57"],
    ]);
    expect(
      within(card)
        .getByText("Car:BlueForester:Insurance")
        .getAttribute("title"),
    ).toBe("Car:BlueForester:Insurance");
    expect(
      within(card)
        .getByRole("heading", { level: 2 })
        .textContent?.replace(/\s+/g, " ")
        .trim(),
    ).toBe("Car costs – Includes past and scheduled transactions");
  });

  it("the gear's Customize… names the card and chooses accounts (open ones first) and spending categories", async () => {
    listsState.categories = [
      { id: 5, name: "Car", parent: null, kind: "expense", hidden: false },
      { id: 7, name: "Gas", parent: 5, kind: "expense", hidden: false },
      { id: 9, name: "Salary", parent: null, kind: "income", hidden: false },
    ] as never;
    sc.update.mockImplementation(
      (id: number, name: string, accounts: number[], categories: number[]) => {
        listsState.spendingCards = [{ id, name, accounts, categories }];
        return ok(listsState.spendingCards[0]);
      },
    );
    shown();
    render(Insights);
    const card = await screen.findByRole("article", { name: "Car costs" });
    await waitFor(() => expect(autoCall).toHaveBeenCalledTimes(1));
    await fireEvent.click(
      within(card).getByRole("button", { name: "Car costs options" }),
    );
    await fireEvent.click(screen.getByRole("menuitem", { name: "Customize…" }));

    const dialog = screen.getByRole("dialog", {
      name: "Customize spending card",
    });
    const name = within(dialog).getByRole("textbox", {
      name: "Name",
    }) as HTMLInputElement;
    expect(name.value).toBe("Car costs");
    await fireEvent.input(name, { target: { value: "Auto" } });
    expect(
      (
        within(dialog).getByRole("checkbox", {
          name: "Checking",
        }) as HTMLInputElement
      ).checked,
    ).toBe(true);
    await fireEvent.click(
      within(dialog).getByRole("checkbox", { name: "Show hidden and closed" }),
    );
    expect(
      (
        within(dialog).getByRole("checkbox", {
          name: "Old Visa",
        }) as HTMLInputElement
      ).checked,
    ).toBe(false);

    await fireEvent.click(
      within(dialog).getByRole("tab", { name: "Categories" }),
    );
    expect(
      (
        within(dialog).getByRole("checkbox", {
          name: "Car",
        }) as HTMLInputElement
      ).checked,
    ).toBe(false);
    expect(
      within(dialog).queryByRole("checkbox", { name: "Salary" }),
    ).toBeNull();
    await fireEvent.click(
      within(dialog).getByRole("checkbox", { name: "Car:Gas" }),
    );
    await fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(sc.update).toHaveBeenCalledWith(4, "Auto", [1], [7]);
    expect(await screen.findByRole("article", { name: "Auto" })).toBeTruthy();
    await waitFor(() => expect(autoCall.mock.calls.length).toBeGreaterThan(1));
  });

  it("a refused name keeps Customize open with the reason", async () => {
    sc.update.mockReturnValue(
      Promise.resolve({
        status: "error",
        error: {
          kind: "invalid",
          message: 'a spending card named "Home" already exists',
        },
      }),
    );
    shown();
    render(Insights);
    const card = await screen.findByRole("article", { name: "Car costs" });
    await fireEvent.click(
      within(card).getByRole("button", { name: "Car costs options" }),
    );
    await fireEvent.click(screen.getByRole("menuitem", { name: "Customize…" }));
    const dialog = screen.getByRole("dialog", {
      name: "Customize spending card",
    });
    await fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
    expect((await within(dialog).findByRole("alert")).textContent).toMatch(
      /already exists/,
    );
  });

  it("Delete card… asks, naming the insights it is on, then takes it off them", async () => {
    const ask = vi.spyOn(confirmState, "ask").mockResolvedValue(true);
    sc.remove.mockImplementation(() => {
      listsState.spendingCards = [];
      listsState.insights = [{ ...auto(), cards: [] }, { ...status }];
      return ok(null);
    });
    shown([auto(), { ...status, cards: ["spending:4"] }]);
    render(Insights);
    const card = await screen.findByRole("article", { name: "Car costs" });
    await fireEvent.click(
      within(card).getByRole("button", { name: "Car costs options" }),
    );
    await fireEvent.click(
      screen.getByRole("menuitem", { name: "Delete card…" }),
    );
    await waitFor(() => expect(sc.remove).toHaveBeenCalledWith(4));
    expect(ask).toHaveBeenCalledWith(
      "Delete the spending card “Car costs”? It will be taken off the insights “Car”, “Status”.",
    );
    await waitFor(() =>
      expect(screen.queryByRole("article", { name: "Car costs" })).toBeNull(),
    );
  });

  it("New spending card… in the insight dialog makes the card and puts it on the insight", async () => {
    sc.create.mockImplementation((name: string) => {
      listsState.spendingCards = [
        carCard(),
        { id: 6, name, accounts: null, categories: [] },
      ];
      return ok(listsState.spendingCards[1]);
    });
    ins.update.mockImplementation((id: number, name: string, cards: string[]) =>
      ok({ id, name, cards }),
    );
    shown();
    render(Insights);
    await screen.findByRole("article", { name: "Car costs" });
    await gear("Customize…");
    const dlg = screen.getByRole("dialog", { name: "Customize insight" });
    expect(within(dlg).getByRole("option", { name: "Car costs" })).toBeTruthy();
    await fireEvent.click(
      within(dlg).getByRole("button", { name: "New spending card…" }),
    );
    await fireEvent.input(
      within(dlg).getByRole("textbox", { name: "Spending card name" }),
      { target: { value: " Home " } },
    );
    await fireEvent.click(within(dlg).getByRole("button", { name: "Create" }));
    await waitFor(() => expect(sc.create).toHaveBeenCalledWith("Home"));
    const right = within(dlg).getByRole("listbox", {
      name: "On this insight (in order)",
    });
    await waitFor(() =>
      expect(within(right).getByRole("option", { name: "Home" })).toBeTruthy(),
    );
    await fireEvent.click(within(dlg).getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(ins.update).toHaveBeenCalledWith(3, "Car", [
        "spending:4",
        "spending:6",
      ]),
    );
  });

  it("is not loaded for an insight without the card", async () => {
    listsState.spendingCards = [carCard()];
    listsState.insights = [spending];
    render(Insights);
    await screen.findAllByText("10,019,506.27");
    expect(autoCall).not.toHaveBeenCalled();
  });
});
