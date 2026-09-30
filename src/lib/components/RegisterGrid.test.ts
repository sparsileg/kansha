import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return {
    ...real,
    commands: {
      registerQuery: vi.fn(),
      registerSummary: vi.fn(),
      accountBalances: vi.fn(),
      payeeSearch: vi.fn(),
      txnSetCleared: vi.fn(),
      entryGet: vi.fn(),
      entryUpdate: vi.fn(),
      payeeList: vi.fn(),
      splitRemainder: vi.fn(),
    },
  };
});

import { commands } from "../api";
import type { RegisterRow } from "../types/bindings";
import { listsState } from "../state/lists.svelte";
import { registerState } from "../state/register.svelte";
import RegisterGrid from "./RegisterGrid.svelte";

const c = vi.mocked(commands, true);
const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });

const row = (id: number, date: string, amount: string, o: object = {}): RegisterRow => ({
  txn_id: id, date, check_num: "", payee: null, payee_name: `P${id}`, memo: "",
  status: "normal" as const, amount, cleared: "unmarked" as const, counterpart: { kind: "none" as const },
  category: "Food", tags: "", balance: "100.00", future: false, ...o,
});

beforeEach(async () => {
  cleanup();
  Element.prototype.scrollIntoView = vi.fn();
  vi.clearAllMocks();
  listsState.today = "2026-09-24";
  // Newest first: two future rows, then two past.
  c.registerQuery.mockImplementation(() =>
    ok({
      rows: [
        row(4, "2026-10-02", "-5.00", { future: true }),
        row(3, "2026-10-01", "20.00", { future: true }),
        row(2, "2026-09-20", "-10.00"),
        row(1, "2026-09-01", "-2.50", { category: "--Split--" }),
      ],
      total: 4,
      today: "2026-09-24",
    }),
  );
  c.registerSummary.mockImplementation(() => ok({ current: "1234.5", cleared: "0.00", ending: "1.00", available_credit: null }));
  c.accountBalances.mockImplementation(() => ok([]));
  c.payeeSearch.mockImplementation(() => ok([]));
  c.txnSetCleared.mockImplementation(() => ok(null));
  c.payeeList.mockImplementation(() => ok([]));
  c.entryUpdate.mockImplementation(() => ok(null));
  c.splitRemainder.mockImplementation(() => ok("0.00"));
  c.entryGet.mockImplementation(() =>
    ok({
      account: 1, date: "2026-10-01", payee: null, check_num: "", memo: "", notes: "",
      amount: "20.00", cleared: "unmarked" as const, tags: [],
      lines: [{ target: { kind: "category" as const, id: 5 }, amount: "20.00", memo: "", cleared: "unmarked" as const, tags: [] }],
    }),
  );
  await registerState.open(1);
});

describe("RegisterGrid", () => {
  it("shows Payment and Deposit columns, split rows, and the footer", async () => {
    render(RegisterGrid, { account: 1 });
    const r1 = document.getElementById("row-1")!;
    expect(r1.textContent).toContain("--Split--");
    expect(r1.textContent).toContain("2.50");
    expect(document.getElementById("row-3")!.textContent).toContain("20.00");
    expect(screen.getByText("1,234.50")).toBeTruthy();
  });

  it("draws one today line where future rows end (REG-070)", () => {
    render(RegisterGrid, { account: 1 });
    const lines = document.querySelectorAll(".today");
    expect(lines).toHaveLength(1);
    expect(lines[0].nextElementSibling?.id).toBe("row-2");
    expect(document.getElementById("row-3")!.classList.contains("future")).toBe(true);
  });

  it("stripes by position; future rows are italic in their own tint; reconciled rows are gray", async () => {
    c.registerQuery.mockImplementation(() =>
      ok({
        rows: [
          row(4, "2026-10-02", "-5.00", { future: true }),
          row(3, "2026-10-01", "20.00", { future: true }),
          row(2, "2026-09-20", "-10.00", { cleared: "cleared" }),
          row(1, "2026-09-01", "-2.50", { cleared: "reconciled", balance: "-2.50" }),
        ],
        total: 4,
        today: "2026-09-24",
      }),
    );
    await registerState.open(1);
    render(RegisterGrid, { account: 1 });
    const cls = (id: number) => [...document.getElementById(`row-${id}`)!.classList].filter((x) => x !== "row" && !x.startsWith("svelte-")).sort();
    // The today line sits between rows 3 and 2 and does not shift the stripes.
    expect(cls(4)).toEqual(["future"]);
    expect(cls(3)).toEqual(["alt", "future"]);
    expect(cls(2)).toEqual([]);
    expect(cls(1)).toEqual(["alt", "reconciled"]);
  });

  it("clicking a header sorts; the today line goes away off date sort", async () => {
    render(RegisterGrid, { account: 1 });
    await fireEvent.click(screen.getByRole("button", { name: /Payee/ }));
    await waitFor(() => expect(c.registerQuery).toHaveBeenLastCalledWith(expect.objectContaining({ sort: "payee", descending: false })));
    await waitFor(() => expect(document.querySelectorAll(".today")).toHaveLength(0));
  });

  it("arrow keys move the selection; space toggles cleared", async () => {
    render(RegisterGrid, { account: 1 });
    const grid = screen.getByRole("grid");
    await fireEvent.keyDown(grid, { key: "ArrowDown" });
    expect(registerState.selected).toBe(4);
    await fireEvent.keyDown(grid, { key: "ArrowDown" });
    expect(registerState.selected).toBe(3);
    await fireEvent.keyDown(grid, { key: " " });
    await waitFor(() => expect(c.txnSetCleared).toHaveBeenCalledWith(3, 1, "cleared", false));
  });

  it("Enter opens the selected row for editing in place", async () => {
    render(RegisterGrid, { account: 1 });
    const grid = screen.getByRole("grid");
    await fireEvent.keyDown(grid, { key: "ArrowDown" });
    await fireEvent.keyDown(grid, { key: "Enter" });
    expect(registerState.editing).toBe(4);
  });

  it("context menu Split opens the row with its split lines, the category as line 1 (REG-080)", async () => {
    render(RegisterGrid, { account: 1 });
    await fireEvent.contextMenu(document.getElementById("row-3")!, { clientX: 50, clientY: 50 });
    const labels = screen.getAllByRole("menuitem").map((b) => b.textContent);
    expect(labels.slice(0, 2)).toEqual(["Edit", "Split"]);
    await fireEvent.click(screen.getByRole("menuitem", { name: "Split" }));
    expect(registerState.editing).toBe(3);
    await waitFor(() => expect(screen.getByLabelText("Split 1 category")).toBeTruthy());
    expect(screen.getByLabelText("Split 2 category")).toBeTruthy();
    expect(registerState.editSplit).toBe(false);
  });

  it("Enter in an edited row saves and moves to the next row; the grid keeps focus", async () => {
    render(RegisterGrid, { account: 1 });
    const grid = screen.getByRole("grid");
    await fireEvent.keyDown(grid, { key: "ArrowDown" }); // row 4
    await fireEvent.keyDown(grid, { key: "ArrowDown" }); // row 3
    await fireEvent.keyDown(grid, { key: "Enter" });
    const form = (await screen.findAllByLabelText("Payment"))[0].closest("form")!;
    await waitFor(() => expect((screen.getAllByLabelText("Deposit")[0] as HTMLInputElement).value).toBe("20.00"));
    await fireEvent.input(screen.getAllByLabelText("Memo")[0], { target: { value: "edited" } });
    await fireEvent.submit(form);
    await waitFor(() => expect(c.entryUpdate).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(registerState.editing).toBeNull());
    expect(registerState.selected).toBe(2);
    await waitFor(() => expect(document.activeElement).toBe(grid));
  });

  it("Esc cancels an edit and keeps the same row selected", async () => {
    render(RegisterGrid, { account: 1 });
    const grid = screen.getByRole("grid");
    await fireEvent.keyDown(grid, { key: "ArrowDown" });
    await fireEvent.keyDown(grid, { key: "Enter" });
    const pay = (await screen.findAllByLabelText("Payment"))[0];
    await fireEvent.keyDown(pay, { key: "Escape" });
    await waitFor(() => expect(registerState.editing).toBeNull());
    expect(registerState.selected).toBe(4);
    expect(c.entryUpdate).not.toHaveBeenCalled();
  });

  it("Enter on an untouched edit writes nothing and still moves to the next row", async () => {
    render(RegisterGrid, { account: 1 });
    const grid = screen.getByRole("grid");
    await fireEvent.keyDown(grid, { key: "ArrowDown" }); // row 4
    await fireEvent.keyDown(grid, { key: "Enter" });
    const dep = (await screen.findAllByLabelText("Deposit"))[0] as HTMLInputElement;
    await waitFor(() => expect(dep.value).toBe("20.00"));
    await fireEvent.submit(dep.closest("form")!);
    await waitFor(() => expect(registerState.editing).toBeNull());
    expect(c.entryUpdate).not.toHaveBeenCalled();
    expect(registerState.selected).toBe(3);
  });

  describe("continuous scroll", () => {
    const many = (n: number) =>
      Array.from({ length: n }, (_, k) => row(n - k, `2026-01-01`, "-1.00"));
    const drawn = () => [...document.querySelectorAll(".row")].map((e) => Number(e.id.slice(4)));
    const spacers = () => [...document.querySelectorAll<HTMLElement>(".spacer-row")].map((e) => e.style.height);
    const scrollTo = async (grid: HTMLElement, top: number) => {
      Object.defineProperty(grid, "scrollTop", { value: top, writable: true, configurable: true });
      await fireEvent.scroll(grid);
    };

    beforeEach(async () => {
      c.registerQuery.mockImplementation(() => ok({ rows: many(1000), total: 1000, today: "2026-09-24" }));
      await registerState.open(1);
      registerState.scrollToEnd = false;
    });

    it("loads every row in one query, with no paging", () => {
      expect(c.registerQuery).toHaveBeenLastCalledWith(expect.objectContaining({ limit: null, offset: 0 }));
      expect(registerState.rows).toHaveLength(1000);
      render(RegisterGrid, { account: 1 });
      expect(screen.queryByRole("button", { name: /Next/ })).toBeNull();
      expect(screen.getByText("1000 transactions")).toBeTruthy();
    });

    it("draws only the rows in view, with spacers for the rest", async () => {
      render(RegisterGrid, { account: 1 });
      const grid = screen.getByRole("grid");
      // 24px rows, an 800px view, 30 rows either side.
      let ids = drawn();
      expect(ids.length).toBeLessThan(100);
      expect(ids[0]).toBe(1000);
      expect(spacers()[0]).toBe("0px");
      await scrollTo(grid, 24 * 500);
      ids = drawn();
      expect(ids[0]).toBe(1000 - 470); // row index 470 = 500 - 30
      expect(spacers()[0]).toBe(`${470 * 24}px`);
      expect(ids.length).toBeLessThan(100);
      // The alternate stripe follows the row's place in the whole list.
      expect(document.getElementById("row-530")!.classList.contains("alt")).toBe(false);
      expect(document.getElementById("row-529")!.classList.contains("alt")).toBe(true);
    });

    it("keeps the row being edited drawn when it scrolls out of view", async () => {
      render(RegisterGrid, { account: 1 });
      const grid = screen.getByRole("grid");
      await fireEvent.keyDown(grid, { key: "ArrowDown" });
      await fireEvent.keyDown(grid, { key: "Enter" });
      expect(registerState.editing).toBe(1000);
      await screen.findAllByLabelText("Payment");
      await scrollTo(grid, 24 * 900);
      expect(screen.getAllByLabelText("Payment").length).toBe(2); // the edit and the new-entry row
    });

    it("reveals a row that is not drawn yet, from its index", async () => {
      render(RegisterGrid, { account: 1 });
      const grid = screen.getByRole("grid");
      Object.defineProperty(grid, "clientHeight", { value: 240, configurable: true });
      registerState.selected = 400; // index 600
      registerState.reveal = 400;
      await waitFor(() => expect(grid.scrollTop).toBe(601 * 24 - 240));
      await waitFor(() => expect(drawn()).toContain(400));
    });

    it("PageDown moves by a screenful", async () => {
      render(RegisterGrid, { account: 1 });
      const grid = screen.getByRole("grid");
      await fireEvent.keyDown(grid, { key: "ArrowDown" });
      await fireEvent.keyDown(grid, { key: "PageDown" });
      // No measured view in the test DOM: 10 rows a page, less one.
      expect(registerState.selected).toBe(1000 - 9);
    });
  });

  it("keeps the saved row pinned in view until the user scrolls", async () => {
    render(RegisterGrid, { account: 1 });
    registerState.reveal = 2;
    await waitFor(() => expect(registerState.reveal).toBe(2));
    await fireEvent.wheel(screen.getByRole("grid"));
    expect(registerState.reveal).toBeNull();
  });
});
