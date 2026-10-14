import { describe, expect, it } from "vitest";
import { buildSave, buildTerms, draftFromDetails, newLoanDraft, rateChanged } from "./form";
import type { LoanDetails } from "../types/bindings";

const TODAY = "2026-01-20";

function filled() {
  const d = newLoanDraft(7);
  d.opening = "2026-01-15";
  d.original = "300,000";
  d.rate = "6";
  d.nextDue = "2026-02-15";
  d.payingAccount = "1";
  d.interestCategory = "9";
  return d;
}

describe("buildSave", () => {
  it("a new loan: years in months, the balance from the original", () => {
    const r = buildSave(filled(), TODAY);
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    const f = r.save.fields;
    expect(f.length_months).toBe(360);
    expect(f.original_balance).toBe("300000.00");
    expect(r.save.current_balance).toBe("300000.00");
    expect(f.rates).toEqual([{ effective: "2026-01-15", rate: "6" }]);
    expect(f.payment).toBeNull();
    expect(f.extra_principal).toBe("0.00");
    expect([f.day1, f.day2]).toEqual([null, null]);
  });

  it("says what is missing", () => {
    const d = filled();
    d.rate = "six";
    expect(buildSave(d, TODAY)).toMatchObject({ ok: false, error: expect.stringContaining("interest rate") });
    d.rate = "6";
    d.payingAccount = "";
    expect(buildSave(d, TODAY)).toMatchObject({ ok: false, error: expect.stringContaining("payments come from") });
    d.payingAccount = "1";
    d.others = [{ category: "4", amount: "" }];
    expect(buildSave(d, TODAY)).toMatchObject({ ok: false, error: expect.stringContaining("Other line") });
    d.others = [{ category: "", amount: "" }];
    expect(buildSave(d, TODAY).ok).toBe(true);
  });

  it("twice per month sends its days; a typed payment is kept", () => {
    const d = filled();
    d.frequency = "twice_per_month";
    d.typed = true;
    d.payment = "1,850.00";
    d.lengthUnit = "months";
    d.length = "180";
    const r = buildSave(d, TODAY);
    expect(r.ok && [r.save.fields.day1, r.save.fields.day2]).toEqual([1, 15]);
    expect(r.ok && r.save.fields.payment).toBe("1850.00");
    expect(r.ok && r.save.fields.length_months).toBe(180);
  });

  it("a changed rate is added from its date (LOAN-050)", () => {
    const details: LoanDetails = {
      loan: {
        fields: {
          account: 7,
          loan_type: "mortgage",
          opening_date: "2026-01-15",
          original_balance: "300000.00",
          length_months: 360,
          compounding: "monthly",
          frequency: "monthly",
          day1: null,
          day2: null,
          rates: [{ effective: "2026-01-15", rate: "6" }],
          payment: null,
          extra_principal: "0.00",
          others: [],
          lender: null,
          paying_account: 1,
          interest_category: 9,
        },
        schedule: 3,
      },
      owed: "299701.35",
      next_due: "2026-03-15",
      view: {
        computed_payment: "1798.65",
        payment: "1798.65",
        next: null,
        others: "0.00",
        total: "1798.65",
        table: [],
        payoff: null,
        total_interest: "0.00",
        payoff_without_extra: null,
        total_interest_without_extra: "0.00",
        short: false,
      },
    };
    const d = draftFromDetails(details, "Big Bank");
    expect(d.current).toBe("299701.35");
    expect(d.lender).toBe("Big Bank");
    expect(rateChanged(d)).toBe(false);
    d.rate = "6.000";
    expect(rateChanged(d)).toBe(false);
    d.rate = "7";
    expect(rateChanged(d)).toBe(true);
    d.rateFrom = "2026-03-01";
    const r = buildSave(d, TODAY);
    expect(r.ok && r.save.fields.rates).toEqual([
      { effective: "2026-01-15", rate: "6" },
      { effective: "2026-03-01", rate: "7" },
    ]);
    expect(r.ok && r.lenderName).toBe("Big Bank");
  });
});

describe("buildTerms", () => {
  it("needs only balance, rate, and length; not the dates or accounts", () => {
    const d = newLoanDraft(7);
    d.original = "30,000";
    d.rate = "5";
    d.length = "60";
    d.lengthUnit = "months";
    expect(buildTerms(d)).toEqual({
      original: "30000.00",
      months: 60,
      rate: "5",
      compounding: "monthly",
      frequency: "monthly",
    });
  });

  it("is null until the terms are typed", () => {
    const d = newLoanDraft(7);
    d.original = "30000";
    expect(buildTerms(d)).toBeNull();
    d.rate = "5";
    d.length = "";
    expect(buildTerms(d)).toBeNull();
  });
});
