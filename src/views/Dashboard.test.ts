import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const openAcct = vi.hoisted(() => vi.fn());

vi.mock("../lib/api", async (orig) => {
  const real = await orig<typeof import("../lib/api")>();
  return {
    ...real,
    commands: {
      appVersion: () => Promise.resolve("0.1.0"),
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
          trend: { dates: ["2026-08-31", "2026-09-27"], series: [{ name: "Net Worth", style: "line", values: ["1.00", "2.00"], pos: [5000, 10000] }], ticks: [{ label: "0", pos: 0 }, { label: "2", pos: 10000 }], zero: 0 },
          upcoming: [{ schedule: 1, nominal: "2026-09-20", date: "2026-09-20", amount: "-50.00", status: "pending", account: 1, payee: null, estimated: false, mode: "remind", overridden: false, txn: null, needs_review: false, overdue: true, actionable: true }],
          upcoming_days: 14,
          warnings: [{ kind: "unreconciled", message: "Checking has uncleared transactions more than 60 days old.", account: 1 }],
        }),
    },
  };
});
vi.mock("../lib/shell/nav", () => ({ openAccount: (id: number) => openAcct(id) }));

import Dashboard from "./Dashboard.svelte";
import { listsState } from "../lib/state/lists.svelte";

beforeEach(() => {
  cleanup();
  vi.clearAllMocks();
  listsState.accounts = [{ id: 1, name: "Checking" }] as never;
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
    await fireEvent.click(screen.getByRole("button", { name: /uncleared/ }));
    expect(openAcct).toHaveBeenCalledWith(1);
  });
});
