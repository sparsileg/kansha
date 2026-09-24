import { describe, expect, it, vi } from "vitest";
import { ApiError, call, withConfirmation } from "./index";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const err = (kind: "invalid" | "confirmation_required", message: string) =>
  Promise.resolve({ status: "error" as const, error: { kind, message } });

describe("call", () => {
  it("returns data on ok", async () => {
    expect(await call(ok(5))).toBe(5);
  });

  it("throws ApiError carrying the kind", async () => {
    const e = (await call(err("invalid", "nope")).catch((x) => x)) as ApiError;
    expect(e).toBeInstanceOf(ApiError);
    expect(e.kind).toBe("invalid");
    expect(e.message).toBe("nope");
  });
});

describe("withConfirmation", () => {
  it("repeats with confirmed=true after the user agrees", async () => {
    const run = vi.fn((c: boolean) =>
      c ? ok("done") : err("confirmation_required", "sure?"),
    );
    const ask = vi.fn(async () => true);
    expect(await withConfirmation(run, ask)).toBe("done");
    expect(ask).toHaveBeenCalledWith("sure?");
    expect(run.mock.calls).toEqual([[false], [true]]);
  });

  it("returns null and does not repeat when declined", async () => {
    const run = vi.fn(() => err("confirmation_required", "sure?"));
    expect(await withConfirmation(run, async () => false)).toBeNull();
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("rethrows other errors without asking", async () => {
    const ask = vi.fn(async () => true);
    await expect(
      withConfirmation(() => err("invalid", "bad"), ask),
    ).rejects.toBeInstanceOf(ApiError);
    expect(ask).not.toHaveBeenCalled();
  });
});
