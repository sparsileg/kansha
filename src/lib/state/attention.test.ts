import { beforeEach, describe, expect, it, vi } from "vitest";

const verify = vi.hoisted(() => vi.fn());
vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return { ...real, commands: { backupVerifyLatest: (p: string) => verify(p) } };
});

import { attentionState, verifyLastBackup } from "./attention.svelte";
import { statusState } from "./status.svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });

beforeEach(() => {
  verify.mockReset();
  statusState.clear();
});

describe("the check of the last backup at startup (BAK-080)", () => {
  it("passes quietly and reloads the card", async () => {
    verify.mockReturnValue(ok({ path: "/b/kansha-1.zip", error: null }));
    const before = attentionState.stamp;
    await verifyLastBackup("secret");
    expect(verify).toHaveBeenCalledWith("secret");
    expect(statusState.message).toBe(null);
    expect(attentionState.stamp).toBe(before + 1);
  });

  it("says nothing when there is no backup", async () => {
    verify.mockReturnValue(ok(null));
    await verifyLastBackup("secret");
    expect(statusState.message).toBe(null);
  });

  it("flashes a failure, and a failed call, without throwing", async () => {
    verify.mockReturnValue(ok({ path: "/b/kansha-1.zip", error: "the passphrase does not open it" }));
    await verifyLastBackup("secret");
    expect(statusState.message).toEqual({
      text: "The check of the last backup failed: the passphrase does not open it.",
      kind: "alert",
    });
    verify.mockReturnValue(Promise.reject(new Error("disk gone")));
    const before = attentionState.stamp;
    await verifyLastBackup("secret");
    expect(statusState.message?.text).toBe("The check of the last backup failed: disk gone");
    expect(attentionState.stamp).toBe(before + 1);
  });
});
