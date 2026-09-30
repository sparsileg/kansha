import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });
const preview = {
  rows: [
    { line: 1, label: "VTI", security: 1, date: "2026-06-30", price: "310.25", replaces: false, skipped: false, error: null },
    { line: 2, label: "ZZZZ", security: null, date: null, price: null, replaces: false, skipped: true, error: null },
  ],
  good: 1,
  errors: 0,
  replaces: 0,
  skipped: 1,
};

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      priceImportPreview: vi.fn(() => ok(preview)),
      priceImport: vi.fn(() => ok(1)),
      securityList: () => ok([]),
    },
  };
});

import { commands } from "../../api";
import PriceImportModal from "./PriceImportModal.svelte";
import { listsState } from "../../state/lists.svelte";

const c = vi.mocked(commands, true);
const file = (text: string, name = "fund-prices.csv") => {
  const f = new File([text], name, { type: "text/csv" });
  // jsdom's File has no text() in every version.
  Object.defineProperty(f, "text", { value: () => Promise.resolve(text) });
  return f;
};

beforeEach(() => {
  cleanup();
  vi.clearAllMocks();
  listsState.today = "2026-06-30";
});

describe("Import prices (PRC-030)", () => {
  it("previews a chosen file with the picked date, then imports", async () => {
    render(PriceImportModal, { onclose: () => {} });
    const input = document.querySelector<HTMLInputElement>('input[type="file"]')!;
    await fireEvent.change(input, { target: { files: [file("VTI,310.25\nZZZZ 1\n")] } });
    await waitFor(() => expect(c.priceImportPreview).toHaveBeenCalledWith("VTI,310.25\nZZZZ 1\n", "2026-06-30"));
    expect(await screen.findByText(/1 good, 0 with problems, 1 skipped/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Import" }));
    await waitFor(() => expect(c.priceImport).toHaveBeenCalledWith("VTI,310.25\nZZZZ 1\n", "2026-06-30"));
    expect(await screen.findByText(/Imported 1 prices from fund-prices.csv/)).toBeTruthy();
  });

  it("takes a file dropped on the drop area", async () => {
    render(PriceImportModal, { onclose: () => {} });
    const area = screen.getByRole("region", { name: "Drop a price file here" });
    await fireEvent.drop(area, { dataTransfer: { files: [file("VTI 311", "prices.txt")] } });
    await waitFor(() => expect(c.priceImportPreview).toHaveBeenCalledWith("VTI 311", "2026-06-30"));
    expect(screen.getByText("prices.txt")).toBeTruthy();
  });

  it("has no Import until a preview is clean", () => {
    render(PriceImportModal, { onclose: () => {} });
    expect((screen.getByRole("button", { name: "Import" }) as HTMLButtonElement).disabled).toBe(true);
  });
});
