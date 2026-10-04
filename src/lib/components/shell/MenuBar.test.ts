import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import MenuBar from "./MenuBar.svelte";
import type { Menu } from "../../shell/menus";

const menus: Menu[] = [
  { id: "a", label: "Alpha", items: [
    { id: "a.one", label: "One" },
    { id: "a.two", label: "Two", disabled: "Planned: Phase 9", divider: true },
    { id: "a.three", label: "Three" },
  ] },
  { id: "b", label: "Beta", items: [
    { id: "b.one", label: "Bee" },
    { id: "b.sub", label: "More", items: [
      { id: "b.sub.x", label: "Ex" },
      { id: "b.sub.y", label: "Why" },
      { id: "b.sub.f", label: "Folder", items: [
        { id: "b.sub.f.r", label: "Report" },
        { id: "b.sub.f.s", label: "Second" },
      ] },
    ] },
  ] },
];

const setup = () => {
  const onselect = vi.fn();
  render(MenuBar, { menus, onselect });
  return onselect;
};

describe("MenuBar", () => {
  it("opens on click, selects an item, and closes", async () => {
    const onselect = setup();
    await fireEvent.click(screen.getByRole("button", { name: "Alpha" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "One" }));
    expect(onselect).toHaveBeenCalledWith("a.one");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("shows a greyed item with its reason, and ignores clicks on it", async () => {
    const onselect = setup();
    await fireEvent.click(screen.getByRole("button", { name: "Alpha" }));
    const two = screen.getByRole("menuitem", { name: /Two/ });
    expect(two.getAttribute("aria-disabled")).toBe("true");
    expect(two.textContent).toContain("Planned: Phase 9");
    await fireEvent.click(two);
    expect(onselect).not.toHaveBeenCalled();
    expect(screen.getByRole("menu")).toBeTruthy(); // still open
  });

  it("moving over another menu switches to it while one is open", async () => {
    setup();
    await fireEvent.click(screen.getByRole("button", { name: "Alpha" }));
    await fireEvent.mouseEnter(screen.getByRole("button", { name: "Beta" }));
    expect(screen.getByRole("menuitem", { name: "Bee" })).toBeTruthy();
    expect(screen.queryByRole("menuitem", { name: "One" })).toBeNull();
  });

  it("keys: Down opens on the first item, arrows skip greyed items, Esc closes", async () => {
    setup();
    const alpha = screen.getByRole("button", { name: "Alpha" });
    alpha.focus();
    await fireEvent.keyDown(alpha, { key: "ArrowDown" });
    const one = await screen.findByRole("menuitem", { name: "One" });
    await waitFor(() => expect(document.activeElement).toBe(one));
    await fireEvent.keyDown(one, { key: "ArrowDown" });
    expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: "Three" }));
    await fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
    expect(document.activeElement).toBe(one);
    await fireEvent.keyDown(one, { key: "ArrowRight" });
    await screen.findByRole("menuitem", { name: "Bee" });
    await fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Beta" }));
  });

  it("a click outside closes it", async () => {
    setup();
    await fireEvent.click(screen.getByRole("button", { name: "Alpha" }));
    await fireEvent.pointerDown(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("a submenu opens on hover or click and runs its items", async () => {
    const onselect = setup();
    await fireEvent.click(screen.getByRole("button", { name: "Beta" }));
    const more = screen.getByRole("menuitem", { name: /More/ });
    expect(more.getAttribute("aria-haspopup")).toBe("menu");
    expect(screen.queryByRole("menuitem", { name: "Ex" })).toBeNull();
    await fireEvent.mouseEnter(more.parentElement!);
    await fireEvent.click(screen.getByRole("menuitem", { name: "Why" }));
    expect(onselect).toHaveBeenCalledWith("b.sub.y");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("submenu keys: Right opens on its first item, Down moves, Left goes back", async () => {
    setup();
    const beta = screen.getByRole("button", { name: "Beta" });
    beta.focus();
    await fireEvent.keyDown(beta, { key: "ArrowDown" });
    const bee = await screen.findByRole("menuitem", { name: "Bee" });
    await waitFor(() => expect(document.activeElement).toBe(bee));
    await fireEvent.keyDown(bee, { key: "ArrowDown" });
    const more = screen.getByRole("menuitem", { name: /More/ });
    expect(document.activeElement).toBe(more);
    await fireEvent.keyDown(more, { key: "ArrowRight" });
    const ex = await screen.findByRole("menuitem", { name: "Ex" });
    await waitFor(() => expect(document.activeElement).toBe(ex));
    await fireEvent.keyDown(ex, { key: "ArrowDown" });
    expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: "Why" }));
    await fireEvent.keyDown(document.activeElement!, { key: "ArrowLeft" });
    expect(screen.queryByRole("menuitem", { name: "Ex" })).toBeNull();
    expect(document.activeElement).toBe(more);
    await fireEvent.keyDown(more, { key: "ArrowRight" });
    await screen.findByRole("menuitem", { name: "Ex" });
    await fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    expect(screen.queryByRole("menuitem", { name: "Ex" })).toBeNull();
    expect(screen.getByRole("menu", { name: "Beta" })).toBeTruthy();
  });

  it("a submenu item opens a third level by mouse or keys", async () => {
    const onselect = setup();
    await fireEvent.click(screen.getByRole("button", { name: "Beta" }));
    await fireEvent.mouseEnter(screen.getByRole("menuitem", { name: /More/ }).parentElement!);
    await fireEvent.mouseEnter(screen.getByRole("menuitem", { name: /Folder/ }).parentElement!);
    expect(screen.getByRole("menu", { name: "Folder" })).toBeTruthy();
    // Moving to a plain item closes the third level.
    await fireEvent.mouseEnter(screen.getByRole("menuitem", { name: "Ex" }));
    expect(screen.queryByRole("menu", { name: "Folder" })).toBeNull();

    // Keys: Right opens it on its first item, Down moves, Left goes back.
    const folder = screen.getByRole("menuitem", { name: /Folder/ });
    folder.focus();
    await fireEvent.keyDown(folder, { key: "ArrowRight" });
    const report = await screen.findByRole("menuitem", { name: "Report" });
    await waitFor(() => expect(document.activeElement).toBe(report));
    await fireEvent.keyDown(report, { key: "ArrowDown" });
    expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: "Second" }));
    await fireEvent.keyDown(document.activeElement!, { key: "ArrowLeft" });
    expect(screen.queryByRole("menu", { name: "Folder" })).toBeNull();
    expect(document.activeElement).toBe(folder);
    // Down in the submenu still reaches its own items, not the third level's.
    await fireEvent.keyDown(folder, { key: "ArrowDown" });
    expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: "Ex" }));

    await fireEvent.click(screen.getByRole("menuitem", { name: /Folder/ }));
    await fireEvent.click(await screen.findByRole("menuitem", { name: "Second" }));
    expect(onselect).toHaveBeenCalledWith("b.sub.f.s");
    expect(screen.queryByRole("menu")).toBeNull();
  });
});
