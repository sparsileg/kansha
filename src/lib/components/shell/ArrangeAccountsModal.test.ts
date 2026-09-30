import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";

const arrange = vi.hoisted(() => vi.fn());
vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      accountArrange: (...a: unknown[]) => (arrange(...a), Promise.resolve({ status: "ok", data: [] })),
    },
  };
});

import ArrangeAccountsModal from "./ArrangeAccountsModal.svelte";
import { listsState } from "../../state/lists.svelte";

const acct = (id: number, name: string, group: string, account_type: string, sort_order = id) =>
  ({ id, name, group, account_type, status: "open", show_in_list: true, sort_order, investment: null }) as never;

beforeEach(() => {
  arrange.mockClear();
  listsState.accounts = [
    acct(1, "Checking", "banking", "checking"),
    acct(2, "Savings", "banking", "savings"),
    acct(3, "HSA", "retirement", "hsa"),
  ];
});

describe("Arrange accounts", () => {
  it("shows all six sections, moves within and between them, and saves runs by group", async () => {
    const onclose = vi.fn();
    render(ArrangeAccountsModal, { onclose });
    for (const h of ["Banking", "Credit", "Investments", "Retirement", "Assets & Debt", "Other"]) {
      expect(screen.getByRole("heading", { name: h })).toBeTruthy();
    }
    await fireEvent.click(screen.getByRole("button", { name: "Move Checking down" }));
    await fireEvent.change(screen.getByRole("combobox", { name: "Section for HSA" }), { target: { value: "other" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(onclose).toHaveBeenCalled());
    expect(arrange).toHaveBeenCalledWith([
      { group: "banking", accounts: [2, 1] },
      { group: "other", accounts: [3] },
    ]);
  });
});
