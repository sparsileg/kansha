import { beforeEach, describe, expect, it } from "vitest";
import { buildRows } from "./rows";
import { dateFormatState } from "../state/dateformat.svelte";
import type { Portfolio } from "../types/bindings";

const totals = {
  basis: "3100.00", market_value: "10050.00", gain: "50.00", day_gain: "150.00",
  day_percent: "5.00", missing_prices: false, stale_prices: false,
};
const pf: Portfolio = {
  as_of: "2026-06-30",
  accounts: [
    {
      account: 2,
      cash: "6900.00",
      totals,
      positions: [
        {
          security: 1, name: "Total Stock Market", ticker: "VTI", shares: "15", basis: "3100.00",
          price: "210", price_date: "2026-06-30", stale: false, market_value: "3150.00", gain: "50.00",
          day_gain: "150.00", day_percent: "5.00",
          lots: [
            { lot: 7, acquired: "2026-02-01", shares: "10", basis: "2000.00", market_value: "2100.00", gain: "100.00", day_gain: "100.00" },
            { lot: 8, acquired: "2026-03-01", shares: "5", basis: "1100.00", market_value: "1050.00", gain: "-50.00", day_gain: null },
          ],
        },
      ],
    },
  ],
  total: totals,
} as never;
const name = () => "Brokerage";

beforeEach(() => dateFormatState.set("mdy"));

describe("investment rows", () => {
  it("collapsed: an account row with its rolled-up figures, then Totals", () => {
    const rows = buildRows(pf, new Set(), name);
    expect(rows.map((r) => r.name)).toEqual(["Brokerage", "Totals:"]);
    expect(rows[0].cells).toEqual({
      cost_basis: "3,100.00", market_value: "10,050.00", gain: "50.00",
      day_gain: "150.00", day_percent: "5.00%",
    });
    expect(rows[1].cells.market_value).toBe("10,050.00");
  });

  it("expanded account: blank header, collapsed equities with all figures, then cash", () => {
    const rows = buildRows(pf, new Set(["a2"]), name);
    expect(rows.map((r) => r.kind)).toEqual(["account", "position", "cash", "total"]);
    expect(rows[0].cells).toEqual({});
    expect(rows[2].cells).toEqual({ market_value: "6,900.00" });
    expect(rows[1].cells).toEqual({
      ticker: "VTI", price: "210.00", shares: "15", cost_basis: "3,100.00",
      market_value: "3,150.00", gain: "50.00", day_gain: "150.00", day_percent: "5.00%",
    });
  });

  it("expanded equity: its lots as 'Lot m/d/yyyy', blank where there is no data", () => {
    const rows = buildRows(pf, new Set(["a2", "p2:1"]), name);
    const lots = rows.filter((r) => r.kind === "lot");
    expect(lots.map((l) => l.name)).toEqual(["Lot 02/01/2026", "Lot 03/01/2026"]);
    expect(lots[0].cells.ticker).toBeUndefined();
    expect(lots[0].cells.day_gain).toBe("100.00");
    expect(lots[1].cells.day_gain).toBe("");
    expect(lots[1].cells.gain).toBe("-50.00");
    expect(rows.find((r) => r.kind === "position")?.cells).toEqual({ ticker: "VTI" });
    // Cash follows the equities' lots.
    expect(rows.map((r) => r.kind).slice(-2)).toEqual(["cash", "total"]);
  });

  it("flags stale and missing prices in words available to a tooltip", () => {
    const stale = structuredClone(pf);
    stale.accounts[0].positions[0].stale = true;
    stale.accounts[0].totals.stale_prices = true;
    const rows = buildRows(stale, new Set(["a2"]), name);
    expect(rows[1].priceWarn).toMatch(/out of date/);
    expect(buildRows(stale, new Set(), name)[0].warn).toMatch(/out of date/);
    const missing = structuredClone(pf);
    missing.total.missing_prices = true;
    expect(buildRows(missing, new Set(), name).at(-1)?.warn).toMatch(/no price/);
  });
});
