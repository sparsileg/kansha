import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return {
    ...real,
    commands: {
      accountBalances: vi.fn(),
      registerQuery: vi.fn(),
      registerSummary: vi.fn(),
      reconcileOpen: vi.fn(),
      reconcileOpeningCheck: vi.fn(),
      reconcileHistory: vi.fn(),
      reconcileSession: vi.fn(),
      reconcileStart: vi.fn(),
      reconcileCheck: vi.fn(),
      reconcileUpdate: vi.fn(),
      reconcileAdjust: vi.fn(),
      reconcileFinish: vi.fn(),
      reconcileAbandon: vi.fn(),
      reconcileHistoryItems: vi.fn(),
    },
  };
});

import { commands } from "../api";
import { confirmState } from "./confirm.svelte";
import { reconcileState } from "./reconcile.svelte";
import { registerState } from "./register.svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const fail = (kind: "invalid" | "confirmation_required", message: string) =>
  Promise.resolve({ status: "error" as const, error: { kind, message } });
const c = vi.mocked(commands, true);

const rec = { id: 5, account: 1, statement_date: "2026-01-31", status: "in_progress" } as never;
const opening = { expected: "0.00", actual: "0.00", changed: [], matches: true };
const session = (difference: string) =>
  ({
    reconciliation: rec,
    payments: [],
    deposits: [],
    opening: "0.00",
    difference,
    opening_check: opening,
  }) as never;

beforeEach(async () => {
  vi.clearAllMocks();
  registerState.close();
  c.accountBalances.mockImplementation(() => ok([]));
  c.reconcileOpeningCheck.mockImplementation(() => ok(opening));
  c.reconcileHistory.mockImplementation(() => ok([]));
  c.reconcileOpen.mockImplementation(() => ok(null));
  await reconcileState.select(null);
});

describe("reconcileState", () => {
  it("loads the opening check and history when no session is open", async () => {
    await reconcileState.select(1);
    expect(reconcileState.session).toBeNull();
    expect(reconcileState.opening).toEqual(opening);
    expect(c.reconcileSession).not.toHaveBeenCalled();
  });

  it("resumes a session in progress", async () => {
    c.reconcileOpen.mockImplementation(() => ok(rec));
    c.reconcileSession.mockImplementation(() => ok(session("500.00")));
    await reconcileState.select(1);
    expect(reconcileState.session?.difference).toBe("500.00");
    expect(c.reconcileSession).toHaveBeenCalledWith(5);
  });

  it("checking items replaces the session with the engine's new figures", async () => {
    c.reconcileOpen.mockImplementation(() => ok(rec));
    c.reconcileSession.mockImplementation(() => ok(session("500.00")));
    await reconcileState.select(1);
    c.reconcileCheck.mockImplementation(() => ok(session("0.00")));
    await reconcileState.check([10, 11], true);
    expect(c.reconcileCheck).toHaveBeenCalledWith(5, [10, 11], true);
    expect(reconcileState.session?.difference).toBe("0.00");
  });

  it("shows an engine refusal and keeps the session", async () => {
    c.reconcileOpen.mockImplementation(() => ok(rec));
    c.reconcileSession.mockImplementation(() => ok(session("5.00")));
    await reconcileState.select(1);
    c.reconcileFinish.mockImplementation(() => fail("invalid", "the difference is 5.00"));
    await reconcileState.finish();
    expect(reconcileState.error).toBe("the difference is 5.00");
    expect(reconcileState.session?.difference).toBe("5.00");
    expect(reconcileState.busy).toBe(false);
  });

  it("a Balance Adjustment repeats with confirmation once the user agrees", async () => {
    c.reconcileOpen.mockImplementation(() => ok(rec));
    c.reconcileSession.mockImplementation(() => ok(session("-5.00")));
    await reconcileState.select(1);
    c.reconcileAdjust.mockImplementation((_id, confirmed) =>
      confirmed ? ok(session("0.00")) : fail("confirmation_required", "adjust by -5.00?"),
    );
    const done = reconcileState.adjust();
    await vi.waitFor(() => expect(confirmState.message).toContain("adjust by -5.00?"));
    confirmState.answer(true);
    await done;
    expect(c.reconcileAdjust.mock.calls.map((x) => x[1])).toEqual([false, true]);
    expect(reconcileState.session?.difference).toBe("0.00");
  });

  it("a declined Balance Adjustment changes nothing", async () => {
    c.reconcileOpen.mockImplementation(() => ok(rec));
    c.reconcileSession.mockImplementation(() => ok(session("-5.00")));
    await reconcileState.select(1);
    c.reconcileAdjust.mockImplementation(() => fail("confirmation_required", "adjust?"));
    const done = reconcileState.adjust();
    await vi.waitFor(() => expect(confirmState.message).not.toBeNull());
    confirmState.answer(false);
    await done;
    expect(c.reconcileAdjust).toHaveBeenCalledTimes(1);
    expect(reconcileState.session?.difference).toBe("-5.00");
  });

  it("abandoning asks first, then reloads without a session", async () => {
    c.reconcileOpen.mockImplementationOnce(() => ok(rec));
    c.reconcileSession.mockImplementation(() => ok(session("5.00")));
    await reconcileState.select(1);
    c.reconcileAbandon.mockImplementation(() => ok(rec));
    c.reconcileOpen.mockImplementation(() => ok(null));

    const declined = reconcileState.abandon();
    await vi.waitFor(() => expect(confirmState.message).not.toBeNull());
    confirmState.answer(false);
    await declined;
    expect(c.reconcileAbandon).not.toHaveBeenCalled();
    expect(reconcileState.session).not.toBeNull();

    const done = reconcileState.abandon();
    await vi.waitFor(() => expect(confirmState.message).not.toBeNull());
    confirmState.answer(true);
    await done;
    expect(c.reconcileAbandon).toHaveBeenCalledWith(5);
    expect(reconcileState.session).toBeNull();
  });

  it("starting refreshes into the new session", async () => {
    await reconcileState.select(1);
    c.reconcileStart.mockImplementation(() => ok(rec));
    c.reconcileOpen.mockImplementation(() => ok(rec));
    c.reconcileSession.mockImplementation(() => ok(session("1000.00")));
    const input = { account: 1 } as never;
    expect(await reconcileState.start(input)).toBe(true);
    expect(c.reconcileStart).toHaveBeenCalledWith(input);
    expect(reconcileState.session?.difference).toBe("1000.00");
  });
});
