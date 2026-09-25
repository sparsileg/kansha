import { describe, expect, it } from "vitest";
import { buildStart, emptyStartForm, isReconcilable } from "./form";

const TODAY = "2026-04-10";
const form = (over: Partial<ReturnType<typeof emptyStartForm>> = {}) => ({
  ...emptyStartForm(),
  statementDate: "3/31/2026",
  balance: "1,234.5",
  ...over,
});

describe("buildStart", () => {
  it("turns typed text into the engine's input", () => {
    const r = buildStart(7, form(), TODAY);
    expect(r).toEqual({
      ok: true,
      input: {
        account: 7,
        statement_date: "2026-03-31",
        statement_balance: "1234.50",
        interest: null,
        service_charge: null,
      },
    });
  });

  it("keeps the sign of a balance owed", () => {
    const r = buildStart(7, form({ balance: "-254" }), TODAY);
    expect(r.ok && r.input.statement_balance).toBe("-254.00");
  });

  it("includes interest and a service charge only when an amount is typed", () => {
    const r = buildStart(
      7,
      form({
        interest: { date: "", amount: "2.5", category: 3 },
        service: { date: "3/30", amount: "5", category: 4 },
      }),
      TODAY,
    );
    expect(r.ok && r.input.interest).toEqual({ date: "2026-03-31", amount: "2.50", category: 3 });
    expect(r.ok && r.input.service_charge).toEqual({ date: "2026-03-30", amount: "5.00", category: 4 });
  });

  it("ignores a category or date typed without an amount", () => {
    const r = buildStart(
      7,
      form({ interest: { date: "1/1", amount: " ", category: 3 } }),
      TODAY,
    );
    expect(r.ok && r.input.interest).toBeNull();
  });

  it("explains what is wrong", () => {
    expect(buildStart(7, form({ statementDate: "soon" }), TODAY)).toEqual({
      ok: false,
      error: "Enter the statement date, like 03/31/2026.",
    });
    expect(buildStart(7, form({ balance: "" }), TODAY)).toMatchObject({ ok: false });
    expect(
      buildStart(7, form({ interest: { date: "", amount: "1.234", category: 3 } }), TODAY),
    ).toEqual({ ok: false, error: "Interest: enter an amount like 12.34." });
    expect(
      buildStart(7, form({ service: { date: "", amount: "5", category: null } }), TODAY),
    ).toEqual({ ok: false, error: "Service charge: choose a category." });
    expect(
      buildStart(7, form({ service: { date: "13/45", amount: "5", category: 4 } }), TODAY),
    ).toEqual({ ok: false, error: "Service charge: enter a date like 03/31/2026." });
  });
});

describe("isReconcilable", () => {
  it("matches the engine's account types", () => {
    for (const t of ["checking", "savings", "cash", "money_market", "credit_card"]) {
      expect(isReconcilable(t)).toBe(true);
    }
    for (const t of ["brokerage", "loan", "other_asset", "other_liability", "roth_ira"]) {
      expect(isReconcilable(t)).toBe(false);
    }
  });
});
