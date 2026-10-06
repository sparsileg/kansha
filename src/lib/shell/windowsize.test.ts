import { beforeEach, describe, expect, it, vi } from "vitest";

const calls: string[] = [];
let inTauri = true;

vi.mock("../api", () => ({
  commands: {
    windowMode: (mode: string) => {
      if (!inTauri) return Promise.reject(new Error("no Tauri"));
      calls.push(mode);
      return Promise.resolve(null);
    },
  },
}));

import { compactWindow, fullWindow } from "./windowsize";

beforeEach(() => {
  calls.length = 0;
  inTauri = true;
});

describe("window size", () => {
  it("asks Rust for the start page or the working size", async () => {
    await compactWindow();
    await fullWindow();
    expect(calls).toEqual(["start", "working"]);
  });

  it("does nothing outside Tauri", async () => {
    inTauri = false;
    await expect(fullWindow()).resolves.toBeUndefined();
    expect(calls).toEqual([]);
  });
});
