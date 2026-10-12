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
    await fireEvent.click(screen.getByLabelText("Copy of this book, without its transactions"));
    await fireEvent.click(screen.getByLabelText(/Carry each balance on/));
    const date = screen.getByLabelText("Carry date") as HTMLInputElement;
    expect(date.value).toBe("10/11/2026");
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
    await fireEvent.click(screen.getByLabelText("Copy of this book, without its transactions"));
    await fireEvent.click(screen.getByLabelText(/Carry each balance on/));
    await fireEvent.input(screen.getByLabelText("Carry date"), { target: { value: "abc" } });
    expect(create().disabled).toBe(true);
    await fireEvent.click(screen.getByLabelText(/Zero/));
    expect(create().disabled).toBe(false);
  });

  it("offers no copy when no book is open", () => {
    bookState.status = { ...bookState.status!, open: false };
    render(NewBookModal, { onclose: () => {} });
    expect(screen.queryByLabelText("Copy of this book, without its transactions")).toBeNull();
  });
});
