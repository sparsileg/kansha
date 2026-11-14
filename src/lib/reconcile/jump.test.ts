import { beforeEach, describe, expect, it, vi } from "vitest";

const nav = vi.hoisted(() => ({ openAccount: vi.fn(), openReconcile: vi.fn() }));
vi.mock("../shell/nav", () => nav);

import { backToReconcile, reconcileEdit, reconcileNew, reconciling } from "./jump";
import { reconcileState } from "../state/reconcile.svelte";
import { registerState } from "../state/register.svelte";

beforeEach(() => {
  vi.restoreAllMocks();
  nav.openAccount.mockReset();
  nav.openReconcile.mockReset();
  reconcileState.accountId = 1;
  reconcileState.session = { reconciliation: { account: 1 } } as never;
  registerState.newEntry = null;
  registerState.editing = null;
});

describe("Reconcile to the register and back (RCN-080)", () => {
  it("New opens the account's register asking for a new entry", async () => {
    await reconcileNew();
    expect(nav.openAccount).toHaveBeenCalledWith(1);
    expect(registerState.newEntry).toEqual({ account: 1 });
  });

  it("a row opens its transaction selected, in its edit", async () => {
    const go = vi.spyOn(registerState, "goToTransaction").mockResolvedValue();
    await reconcileEdit(42);
    expect(nav.openAccount).toHaveBeenCalledWith(1);
    expect(go).toHaveBeenCalledWith(1, 42);
    expect(registerState.editing).toBe(42);
  });

  it("the register offers Back to Reconcile only for the account with the session", async () => {
    expect(reconciling(1)).toBe(true);
    expect(reconciling(2)).toBe(false);
    reconcileState.session = null;
    expect(reconciling(1)).toBe(false);
    reconcileState.session = { reconciliation: { account: 1 } } as never;
    await backToReconcile();
    expect(nav.openReconcile).toHaveBeenCalledWith(1);
  });
});
