import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import Dock from "../components/shell/Dock.svelte";
import { viewState } from "./view.svelte";
import { windowState } from "./windows.svelte";

beforeEach(() => {
  cleanup();
  windowState.reset();
  viewState.reset();
});

describe("windows", () => {
  it("showing a window is a view; going elsewhere leaves it open", () => {
    viewState.navigate("search");
    const id = windowState.add("test", () => "Net Worth");
    windowState.show(id);
    expect(windowState.shown).toBe(id);
    viewState.navigate("manage");
    expect(windowState.shown).toBeNull();
    expect(windowState.wins).toHaveLength(1);
    viewState.back();
    expect(windowState.shown).toBe(id);
  });

  it("minimize shows the view under the windows", () => {
    viewState.navigate("search");
    const a = windowState.add("test", () => "A");
    const b = windowState.add("test", () => "B");
    windowState.show(a);
    windowState.show(b);
    windowState.minimize();
    expect(viewState.current).toBe("search");
  });

  it("closing drops the window from history", async () => {
    viewState.navigate("search");
    const a = windowState.add("test", () => "A");
    windowState.show(a);
    expect(await windowState.close(a)).toBe(true);
    expect(viewState.current).toBe("search");
    expect(viewState.entries.map((e) => e.view)).toEqual(["dashboard", "search"]);
  });

  it("a kind's hook can keep a window open", async () => {
    const closed = vi.fn();
    windowState.register("stubborn", { beforeClose: () => Promise.resolve(false), closed });
    const a = windowState.add("stubborn", () => "A");
    expect(await windowState.close(a)).toBe(false);
    expect(windowState.wins).toHaveLength(1);
    windowState.register("stubborn", { closed });
    await windowState.close(a);
    expect(closed).toHaveBeenCalledWith(a);
  });

  it("a panel has one window; opening it again shows that one", () => {
    const a = windowState.openSingle("calendar", "Calendar");
    viewState.navigate("search");
    const b = windowState.openSingle("calendar", "Calendar");
    expect(b).toBe(a);
    expect(windowState.wins).toHaveLength(1);
    expect(windowState.shownKind).toBe("calendar");
  });

  it("before quitting, every kind is asked and nothing closes", async () => {
    const asked: number[] = [];
    windowState.register("careful", { beforeClose: (id) => (asked.push(id), Promise.resolve(id !== b)) });
    const a = windowState.add("careful", () => "A");
    const b = windowState.add("careful", () => "B");
    const c = windowState.add("careful", () => "C");
    expect(await windowState.mayQuit()).toBe(false);
    expect(asked).toEqual([a, b]); // stops at the first refusal
    expect(windowState.wins).toHaveLength(3);
    windowState.register("careful", { beforeClose: () => Promise.resolve(true) });
    expect(await windowState.mayQuit()).toBe(true);
    expect(windowState.wins.map((w) => w.id)).toEqual([a, b, c]);
  });

  it("repeated names are numbered", () => {
    const a = windowState.add("test", () => "Net Worth");
    const b = windowState.add("test", () => "Net Worth");
    const c = windowState.add("test", () => "Tax Summary");
    const labels = windowState.labels();
    expect([labels.get(a), labels.get(b), labels.get(c)]).toEqual(["Net Worth", "Net Worth (2)", "Tax Summary"]);
  });
});

describe("Dock", () => {
  it("lists windows by name; a click shows, a second minimizes, × closes", async () => {
    viewState.navigate("search");
    windowState.add("test", () => "Net Worth");
    windowState.add("test", () => "Tax Summary");
    render(Dock);
    const tax = screen.getByRole("button", { name: "Tax Summary" });
    await fireEvent.click(tax);
    expect(tax.getAttribute("aria-current")).toBe("page");
    await fireEvent.click(tax);
    expect(viewState.current).toBe("search");
    await fireEvent.click(screen.getByRole("button", { name: "Close Net Worth" }));
    expect(screen.queryByRole("button", { name: "Net Worth" })).toBeNull();
  });

  it("is hidden with no windows", () => {
    render(Dock);
    expect(screen.queryByRole("navigation", { name: "Open windows" })).toBeNull();
  });
});
