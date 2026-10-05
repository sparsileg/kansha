import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/svelte";
import { tick } from "svelte";

vi.mock("../lib/api", async (orig) => {
  const real = await orig<typeof import("../lib/api")>();
  return { ...real, commands: { ...real.commands, bookRecent: () => Promise.resolve([]) } };
});

const sizes: string[] = [];
vi.mock("../lib/shell/windowsize", () => ({
  compactWindow: () => (sizes.push("compact"), Promise.resolve()),
  fullWindow: () => (sizes.push("full"), Promise.resolve()),
}));

import StartScreen from "./StartScreen.svelte";
import { bookState } from "../lib/state/book.svelte";
import type { BookState } from "../lib/types/bindings";

function status(state: BookState) {
  bookState.status = {
    state,
    open: false,
    name: "import",
    db_path: "/books/import.db",
    folder: "/books",
    downloads: null,
  };
}

beforeEach(() => {
  cleanup();
  sizes.length = 0;
});

describe("start screen", () => {
  it("shows the mark and what kansha means", () => {
    status("locked");
    render(StartScreen);
    expect(screen.getByRole("img", { name: /kansha/ })).toBeTruthy();
    expect(screen.getByText("Gratitude")).toBeTruthy();
  });

  it("asks for the passphrase in a compact window", async () => {
    status("locked");
    render(StartScreen);
    await tick();
    expect(sizes).toEqual(["compact"]);
    expect(screen.getByLabelText("Backup passphrase")).toBeTruthy();
  });

  it("grows the window for first-run setup", async () => {
    status("new");
    render(StartScreen);
    await tick();
    expect(sizes).toEqual(["full"]);
  });

  it("grows the window to create a new book from the passphrase screen", async () => {
    status("locked");
    render(StartScreen);
    await tick();
    screen.getByText("Create a new book…").click();
    await tick();
    expect(sizes).toEqual(["compact", "full"]);
  });
});
