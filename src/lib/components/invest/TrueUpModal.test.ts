import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";

const { trueUpPreview, trueUp } = vi.hoisted(() => {
  const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });
  const preview = (account: number, security: number, date: string) => ({
    account,
    security,
    date,
    lines: [{ status: "close", lot: 1, acquired: "2022-01-10", quantity: "10", basis: "3000.00" }],
    kansha_quantity: "10",
    kansha_basis: "3000.00",
    broker_quantity: "10",
    broker_basis: "1000.00",
    basis_compared: true,
    changes: true,
    replayed: [],
    problem: null,
  });
  return {
    trueUpPreview: vi.fn((a: number, s: number, d: string, _text: string) => ok(preview(a, s, d))),
    trueUp: vi.fn((_a: number, _s: number, d: string, _text: string, _memo: string) =>
      ok({ date: d, disposals: [{}], lots: [{}] }),
    ),
  };
});

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return { ...real, commands: { trueUpPreview, trueUp } };
});

import TrueUpModal from "./TrueUpModal.svelte";
import { investState } from "../../state/invest.svelte";
import { listsState } from "../../state/lists.svelte";

beforeEach(() => {
  cleanup();
  trueUpPreview.mockClear();
  trueUp.mockClear();
  listsState.today = "2026-10-01";
  listsState.accounts = [
    { id: 15, name: "Brokerage", investment: {}, status: "open" },
    { id: 16, name: "Other", investment: {}, status: "open" },
  ] as never;
  investState.securities = [{ id: 49, name: "Total", ticker: "VTI", hidden: false }] as never;
  vi.spyOn(investState, "refresh").mockResolvedValue();
});

async function compared() {
  render(TrueUpModal, { onclose: () => {} });
  await fireEvent.change(screen.getByLabelText("Account"), { target: { value: "15" } });
  await fireEvent.change(screen.getByLabelText("Security"), { target: { value: "49" } });
  await fireEvent.input(screen.getByLabelText(/True-up date/), { target: { value: "2025-12-31" } });
  await fireEvent.input(screen.getByLabelText(/paste the CSV/), { target: { value: "date,shares,basis\n" } });
  await fireEvent.click(screen.getByRole("button", { name: "Compare" }));
  await screen.findByRole("cell", { name: "Close" });
}

const trueUpButton = () => screen.getByRole("button", { name: "True up" }) as HTMLButtonElement;

describe("TrueUpModal (MIG-115)", () => {
  it("applies the comparison with the choices it was made with", async () => {
    await compared();
    expect(trueUpButton().disabled).toBe(false);
    await fireEvent.click(trueUpButton());
    await screen.findByText(/Lots trued up/);
    expect(trueUp).toHaveBeenCalledWith(15, 49, "2025-12-31", "date,shares,basis\n", "Lot true-up to broker");
  });

  for (const [what, change] of [
    ["account", () => fireEvent.change(screen.getByLabelText("Account"), { target: { value: "16" } })],
    ["date", () => fireEvent.input(screen.getByLabelText(/True-up date/), { target: { value: "2025-12-30" } })],
    ["text", () => fireEvent.input(screen.getByLabelText(/paste the CSV/), { target: { value: "other" } })],
  ] as const) {
    it(`changing the ${what} hides the comparison until Compare runs again`, async () => {
      await compared();
      await change();
      expect(screen.queryByRole("cell", { name: "Close" })).toBeNull();
      expect(trueUpButton().disabled).toBe(true);
      await fireEvent.click(screen.getByRole("button", { name: "Compare" }));
      await screen.findByRole("cell", { name: "Close" });
      expect(trueUpButton().disabled).toBe(false);
    });
  }
});
