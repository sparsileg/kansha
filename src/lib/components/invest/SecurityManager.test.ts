import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const update = vi.hoisted(() => vi.fn());
const sec = (id: number, name: string, mix: { asset_class: string; percent: string }[] = []) => ({
  id, name, ticker: null, security_type: "mutual_fund", asset_class: "us_equity", class_mix: mix,
  cusip: null, default_lot_method: null, hidden: false, notes: "", created_at: "",
});

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      securityList: () =>
        ok([
          sec(1, "Stock Fund"),
          sec(2, "Balanced", [
            { asset_class: "us_equity", percent: "60" },
            { asset_class: "bond_intermediate", percent: "40" },
          ]),
        ]),
      priceList: () => ok([]),
      securityUpdate: (...a: unknown[]) => (update(...a), ok(sec(1, "Stock Fund"))),
    },
  };
});

import SecurityManager from "./SecurityManager.svelte";
import { listsState } from "../../state/lists.svelte";

beforeEach(() => {
  update.mockClear();
  listsState.today = "2026-06-30";
});

describe("Securities: class mix (SEC-050)", () => {
  it("lists a mixed security's classes and percents", async () => {
    render(SecurityManager);
    expect(await screen.findByText("US equity 60%, Intermed bonds 40%")).toBeTruthy();
  });

  it("Split across classes replaces the class with a list that is saved", async () => {
    render(SecurityManager);
    await fireEvent.click(await screen.findByText("Stock Fund"));
    expect(screen.getByRole("combobox", { name: "Asset class" })).toBeTruthy();
    await fireEvent.click(screen.getByRole("checkbox", { name: "Split across classes" }));
    expect(screen.queryByRole("combobox", { name: "Asset class" })).toBeNull();
    expect((screen.getByRole("combobox", { name: "Class 1" }) as HTMLSelectElement).value).toBe("us_equity");
    expect((screen.getByRole("combobox", { name: "Class 2" }) as HTMLSelectElement).value).toBe("us_small_mid");
    await fireEvent.input(screen.getByRole("textbox", { name: "Percent 1" }), { target: { value: "70" } });
    await fireEvent.input(screen.getByRole("textbox", { name: "Percent 2" }), { target: { value: "30" } });
    await fireEvent.click(screen.getByRole("button", { name: "Add class" }));
    expect(screen.getByRole("combobox", { name: "Class 3" })).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Remove class 3" }));
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(update).toHaveBeenCalled());
    expect(update.mock.calls[0][1].class_mix).toEqual([
      { asset_class: "us_equity", percent: "70" },
      { asset_class: "us_small_mid", percent: "30" },
    ]);
  });
});
