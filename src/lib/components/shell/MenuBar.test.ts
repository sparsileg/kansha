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
  { id: "b", label: "Beta", items: [{ id: "b.one", label: "Bee" }] },
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
});
