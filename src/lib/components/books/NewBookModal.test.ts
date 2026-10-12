import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";

const bookNew = vi.fn();
vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      bookNew: (...a: unknown[]) => {
        bookNew(...a);
        return Promise.resolve({ status: "ok" as const, data: null });
      },
      pickFolder: () => Promise.resolve(null),
    },
  };
});
vi.mock("../../shell/books", () => ({ reloadForBook: vi.fn() }));

import NewBookModal from "./NewBookModal.svelte";
import { bookState } from "../../state/book.svelte";
import { listsState } from "../../state/lists.svelte";
import { windowState } from "../../state/windows.svelte";

beforeEach(() => {
  cleanup();
  bookNew.mockClear();
  listsState.today = "2026-10-11";
  bookState.status = {
    state: "locked",
    open: true,
    name: "barton2026",
    db_path: "/b/barton2026.db",
    folder: "/b",
    downloads: "/home/u/Downloads",
  };
  vi.spyOn(windowState, "mayQuit").mockResolvedValue(true);
});

async function fill() {
  await fireEvent.input(screen.getByLabelText("Name"), { target: { value: "fresh" } });
  await fireEvent.input(screen.getByLabelText("Passphrase"), { target: { value: "p" } });
  await fireEvent.input(screen.getByLabelText("Again"), { target: { value: "p" } });
}
const create = () => screen.getByRole("button", { name: "Create book" }) as HTMLButtonElement;

describe("NewBookModal (UI-085)", () => {
  it("starts empty unless another start is chosen", async () => {
    render(NewBookModal, { onclose: () => {} });
    await fill();
    await fireEvent.click(create());
    await waitFor(() => expect(bookNew).toHaveBeenCalled());
    expect(bookNew.mock.calls[0][4]).toEqual({ kind: "empty" });
  });

  it("can start with the Standard categories", async () => {
    render(NewBookModal, { onclose: () => {} });
    await fill();
    await fireEvent.click(screen.getByLabelText("Standard categories"));
    await fireEvent.click(create());
    await waitFor(() => expect(bookNew.mock.calls[0][4]).toEqual({ kind: "standard" }));
  });

  it("a copy carries balances on a date and can drop closed accounts", async () => {
    render(NewBookModal, { onclose: () => {} });
    await fill();
    await fireEvent.click(screen.getByLabelText("Copy of this book"));
    await fireEvent.click(screen.getByLabelText(/Carry each balance/));
    const date = screen.getByLabelText("Carry date") as HTMLInputElement;
    expect(date.value).toBe("12/31/2025");
    await fireEvent.input(date, { target: { value: "9/30" } });
    await fireEvent.blur(date);
    expect(date.value).toBe("09/30/2026");
    await fireEvent.click(screen.getByLabelText("Copy closed accounts"));
    await fireEvent.click(create());
    await waitFor(() =>
      expect(bookNew.mock.calls[0][4]).toEqual({
        kind: "copy",
        balances: { kind: "carry", as_of: "2026-09-30" },
        closed_accounts: false,
      }),
    );
  });

  it("a bad carry date blocks Create; Zero needs no date", async () => {
    render(NewBookModal, { onclose: () => {} });
    await fill();
    await fireEvent.click(screen.getByLabelText("Copy of this book"));
    await fireEvent.click(screen.getByLabelText(/Carry each balance/));
    await fireEvent.input(screen.getByLabelText("Carry date"), { target: { value: "abc" } });
    expect(create().disabled).toBe(true);
    await fireEvent.click(screen.getByLabelText(/Zero/));
    expect(create().disabled).toBe(false);
  });

  it("a year-end copy carries to the date and keeps later transactions", async () => {
    render(NewBookModal, { onclose: () => {} });
    await fill();
    await fireEvent.click(screen.getByLabelText("Copy of this book"));
    await fireEvent.click(screen.getByLabelText(/keep transactions after the carry date/));
    const date = screen.getByLabelText("Carry date") as HTMLInputElement;
    expect(date.disabled).toBe(false);
    await fireEvent.input(date, { target: { value: "12/31/2025" } });
    await fireEvent.click(create());
    await waitFor(() =>
      expect(bookNew.mock.calls[0][4]).toEqual({
        kind: "copy",
        balances: { kind: "carry_keep", as_of: "2025-12-31" },
        closed_accounts: true,
      }),
    );
  });

  it("the carry date's line sits under the Carry choice made, and none with Zero", async () => {
    render(NewBookModal, { onclose: () => {} });
    await fireEvent.click(screen.getByLabelText("Copy of this book"));
    expect(screen.queryByLabelText("Carry date")).toBeNull();
    const carry = screen.getByLabelText(/Carry each balance/).closest("label")!;
    const keep = screen.getByLabelText(/keep transactions after the carry date/).closest("label")!;
    await fireEvent.click(carry.querySelector("input")!);
    expect(carry.nextElementSibling?.contains(screen.getByLabelText("Carry date"))).toBe(true);
    await fireEvent.input(screen.getByLabelText("Carry date"), { target: { value: "12/31/2023" } });
    await fireEvent.click(keep.querySelector("input")!);
    const date = screen.getByLabelText("Carry date") as HTMLInputElement;
    expect(keep.nextElementSibling?.contains(date)).toBe(true);
    expect(date.value).toBe("12/31/2023");
    expect(screen.getAllByLabelText("Carry date")).toHaveLength(1);
    await fireEvent.click(screen.getByLabelText(/Zero/));
    expect(screen.queryByLabelText("Carry date")).toBeNull();
  });

  it("every Start with and Balances choice has a tooltip", async () => {
    render(NewBookModal, { onclose: () => {} });
    await fireEvent.click(screen.getByLabelText("Copy of this book"));
    const radios = screen.getAllByRole("radio") as HTMLInputElement[];
    expect(radios).toHaveLength(6);
    // Each ends with when to choose it.
    for (const r of radios) expect(r.closest("label")?.title.split(/(?<=\.) /).at(-1), r.value).toMatch(/^Choose this .+\.$/);
    await fireEvent.click(screen.getByLabelText(/Carry each balance/));
    expect(screen.getByLabelText("Carry date").title).toMatch(/\S/);
    expect(screen.getByLabelText("Copy closed accounts").closest("label")?.title).toMatch(/Turn this off [^.]+\.$/);
    expect(screen.getByLabelText(/Carry each balance/).closest("label")?.title).toContain("such as 12/31/2025");
    expect(screen.getByLabelText(/Carry each balance/).closest("label")?.title).toMatch(/lot/);
  });

  it("offers no copy when no book is open", () => {
    bookState.status = { ...bookState.status!, open: false };
    render(NewBookModal, { onclose: () => {} });
    expect(screen.queryByLabelText("Copy of this book")).toBeNull();
  });
});
