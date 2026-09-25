import { describe, expect, it } from "vitest";
import { ACTIONS, actionInfo, buildInput, emptyForm, formFromInput } from "./form";
import type { InvInput } from "../types/bindings";

const today = "2026-06-30";

describe("action list", () => {
  it("covers every investment action once (INV-010)", () => {
    const values = ACTIONS.map((a) => a.value);
    expect(new Set(values).size).toBe(20);
    expect(actionInfo("split").split).toBe(true);
    expect(actionInfo("cash_in").security).toBe("none");
    expect(actionInfo("sell").lots).toBe(true);
    expect(actionInfo("buy").commission).toBe(true);
    expect(actionInfo("dividend").commission).toBe(false);
  });
});

describe("buildInput", () => {
  it("a buy: canonical strings, commission, no amount means shares × price", () => {
    const f = { ...emptyForm("buy", "1/10/2026"), security: 3, quantity: "1,000", price: "$41.25", commission: "4.95" };
    const r = buildInput(7, f, today);
    expect(r).toEqual({
      ok: true,
      input: expect.objectContaining({
        account: 7,
        action: "buy",
        date: "2026-01-10",
        security: 3,
        quantity: "1000",
        price: "41.25",
        commission: "4.95",
        amount: null,
        lots: [],
        lot_method: null,
      }),
    });
  });

  it("a sale from chosen lots sends each lot's shares", () => {
    const f = {
      ...emptyForm("sell", "2026-03-01"),
      security: 3,
      quantity: "9",
      amount: "2,700",
      lotMethod: "specific" as const,
      picks: { 11: "5", 12: "4", 13: "" },
    };
    const r = buildInput(7, f, today);
    expect(r.ok && r.input.lots).toEqual([
      { lot: 11, quantity: "5" },
      { lot: 12, quantity: "4" },
    ]);
    expect(r.ok && r.input.lot_method).toBe("specific");
    expect(r.ok && r.input.amount).toBe("2700.00");
  });

  it("a split, a transfer, cash in, and fields an action does not use are left out", () => {
    const split = buildInput(7, { ...emptyForm("split", "2026-06-01"), security: 3, splitNew: "4", splitOld: "1", amount: "5" }, today);
    expect(split.ok && split.input.split).toEqual({ new: 4, old: 1 });
    expect(split.ok && split.input.amount).toBeNull();

    const move = buildInput(7, { ...emptyForm("transfer_shares", "2026-06-01"), security: 3, quantity: "2", toAccount: 8 }, today);
    expect(move.ok && move.input.to_account).toBe(8);

    const cash = buildInput(7, { ...emptyForm("cash_in", "2026-06-01"), security: 3, amount: "100", counterpart: "a:1" }, today);
    expect(cash.ok && cash.input.security).toBeNull();
    expect(cash.ok && cash.input.counterpart).toEqual({ kind: "account", id: 1 });
  });

  it("says what does not parse", () => {
    const bad = (over: object, action = "buy" as const) =>
      buildInput(7, { ...emptyForm(action, "2026-06-01"), security: 3, quantity: "1", ...over }, today);
    expect(bad({ date: "13/45" })).toMatchObject({ ok: false, error: expect.stringContaining("date") });
    expect(bad({ security: null })).toMatchObject({ ok: false, error: "Choose a security." });
    expect(bad({ quantity: "-1" })).toMatchObject({ ok: false, error: expect.stringContaining("shares") });
    expect(bad({ price: "abc" })).toMatchObject({ ok: false, error: expect.stringContaining("price") });
    expect(bad({ splitNew: "2", splitOld: "0" }, "split" as never)).toMatchObject({ ok: false, error: expect.stringContaining("split") });
  });
});

describe("formFromInput", () => {
  it("shows a stored sale as typed and builds it back the same", () => {
    const stored: InvInput = {
      account: 7,
      action: "sell",
      date: "2026-03-01",
      settle_date: "2026-03-03",
      security: 3,
      quantity: "1000.5",
      price: "41.2",
      commission: "4.95",
      amount: "2700.00",
      split: null,
      to_account: null,
      lot_method: "specific",
      lots: [{ lot: 11, quantity: "1000.5" }],
      acquired: null,
      counterpart: null,
      memo: "rebalance",
    };
    const f = formFromInput(stored);
    expect(f.quantity).toBe("1,000.5");
    expect(f.price).toBe("41.20");
    expect(f.date).toBe("03/01/2026");
    expect(buildInput(7, f, today)).toEqual({ ok: true, input: stored });
  });
});
