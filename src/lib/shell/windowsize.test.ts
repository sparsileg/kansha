import { beforeEach, describe, expect, it, vi } from "vitest";

const calls: string[] = [];
let maximized = false;
let saved = false;

vi.mock("../api", () => ({
  commands: {
    windowSave: () => (calls.push("save"), Promise.resolve(null)),
    windowRestore: () => (calls.push("restore"), Promise.resolve(saved)),
  },
}));

vi.mock("@tauri-apps/api/window", () => ({
  LogicalSize: class {
    constructor(
      public width: number,
      public height: number,
    ) {}
  },
  getCurrentWindow: () => ({
    isMaximized: () => Promise.resolve(maximized),
    setSize: (s: { width: number; height: number }) => (calls.push(`size ${s.width}x${s.height}`), Promise.resolve()),
    setMinSize: (s: { width: number; height: number }) => (calls.push(`min ${s.width}x${s.height}`), Promise.resolve()),
    center: () => (calls.push("center"), Promise.resolve()),
  }),
}));

// Fresh module state (the window starts compact) for each test.
async function load() {
  vi.resetModules();
  return import("./windowsize");
}

beforeEach(() => {
  calls.length = 0;
  maximized = false;
  saved = false;
});

describe("window size", () => {
  it("starts compact, so only growing does anything at first", async () => {
    const w = await load();
    await w.compactWindow();
    expect(calls).toEqual([]);
    await w.fullWindow();
    // Grow first, then raise the minimum past the compact size.
    expect(calls).toEqual(["restore", "size 1280x800", "min 900x600", "center"]);
  });

  it("shrinks after growing, lowering the minimum first", async () => {
    const w = await load();
    await w.fullWindow();
    calls.length = 0;
    await w.compactWindow();
    expect(calls).toEqual(["save", "min 460x600", "size 520x700", "center"]);
  });

  it("each size is set once, not on every call", async () => {
    const w = await load();
    await w.fullWindow();
    await w.fullWindow();
    expect(calls.filter((c) => c.startsWith("size"))).toEqual(["size 1280x800"]);
  });

  it("leaves a maximized window alone", async () => {
    maximized = true;
    const w = await load();
    await w.fullWindow();
    expect(calls).toEqual(["restore"]);
  });

  it("grows to the saved size and place when there is one", async () => {
    saved = true;
    const w = await load();
    await w.fullWindow();
    // Rust put it back; only the minimum is raised.
    expect(calls).toEqual(["restore", "min 900x600"]);
  });

  it("remembers the working size before shrinking", async () => {
    const w = await load();
    await w.fullWindow();
    calls.length = 0;
    await w.compactWindow();
    expect(calls[0]).toBe("save");
  });
});
