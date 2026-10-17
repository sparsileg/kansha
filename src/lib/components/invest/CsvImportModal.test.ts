import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const preview = {
  date: "2026-06-30",
  rows: [],
  totals: [],
  good: 1,
  errors: 0,
};

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      lotSeedPreview: vi.fn(() => ok(preview)),
      lotSeed: vi.fn(),
      securityList: () => ok([]),
    },
  };
});

vi.mock("../../state/invest.svelte", () => ({
  investState: {
    loadSecurities: vi.fn(() => Promise.resolve()),
    refresh: vi.fn(() => Promise.resolve()),
  },
}));

import { commands } from "../../api";
import CsvImportModal from "./CsvImportModal.svelte";
import { listsState } from "../../state/lists.svelte";

const c = vi.mocked(commands, true);

beforeEach(() => {
  cleanup();
  vi.clearAllMocks();
  listsState.today = "2026-06-30";
});

async function previewed() {
  render(CsvImportModal, { onclose: () => {} });
  await fireEvent.input(screen.getByLabelText("Or paste the CSV"), {
    target: { value: "Brokerage,VTI,2020-01-02,10,1000.00\n" },
  });
  await fireEvent.click(screen.getByRole("button", { name: "Preview" }));
  await screen.findByText(/1 lots good/);
  return screen.getByRole("button", { name: "Import" }) as HTMLButtonElement;
}

describe("Seed lots from CSV (MIG-120)", () => {
  it("a second click while importing seeds once", async () => {
    let release!: () => void;
    c.lotSeed.mockImplementation(
      () => new Promise((r) => (release = () => r({ status: "ok", data: 1 }))),
    );
    const button = await previewed();
    await fireEvent.click(button);
    expect(button.disabled).toBe(true);
    await fireEvent.click(button);
    release();
    await screen.findByText(/Created 1 lots/);
    expect(c.lotSeed).toHaveBeenCalledTimes(1);
  });

  it("a failed import can be tried again", async () => {
    c.lotSeed.mockImplementation(
      () =>
        Promise.resolve({
          status: "error",
          error: { kind: "invalid", message: "bad line 1" },
        }) as never,
    );
    const button = await previewed();
    await fireEvent.click(button);
    await screen.findByRole("alert");
    expect(button.disabled).toBe(false);
  });
});
