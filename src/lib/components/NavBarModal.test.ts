import { beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/svelte";
import NavBarModal from "./NavBarModal.svelte";
import { parseNav } from "../shell/navitems";
import { loadPref } from "../state/prefs";
import { dialogState } from "../state/dialogs.svelte";
import { listsState } from "../state/lists.svelte";
import { settingsState } from "../state/settings.svelte";

const shown = () => within(screen.getByLabelText("On the bar (left to right)")).getAllByRole("option").map((o) => o.textContent?.trim());
const available = () => within(screen.getByLabelText("Available")).getAllByRole("option").map((o) => o.textContent?.trim());
const choose = (label: string, value: string) => fireEvent.change(screen.getByLabelText(label), { target: { value } });

beforeEach(() => {
  cleanup();
  localStorage.clear();
  listsState.accounts = [{ id: 1, name: "Checking" }] as never;
  settingsState.setNavItems(["home", "tools.calendar"]);
  dialogState.navbar = true;
});

describe("Navigation bar dialog", () => {
  it("lists what is on the bar in order, and what else is available", () => {
    render(NavBarModal);
    expect(shown()).toEqual(["Home", "Calendar"]);
    expect(available()).toContain("Checking");
    expect(available()).toContain("Reconcile (Planned: Phase 5)");
    expect(available()).not.toContain("Home");
  });

  it("adds, moves, and removes; Save keeps the result and closes", async () => {
    render(NavBarModal);
    await choose("Available", "account:1");
    await fireEvent.click(screen.getByRole("button", { name: "Add ›" }));
    expect(shown()).toEqual(["Home", "Calendar", "Checking"]);
    expect(available()).not.toContain("Checking");
    // The added item is selected on the right; move it to the front.
    await fireEvent.click(screen.getByRole("button", { name: /Move up/ }));
    await fireEvent.click(screen.getByRole("button", { name: /Move up/ }));
    expect(shown()).toEqual(["Checking", "Home", "Calendar"]);
    expect((screen.getByRole("button", { name: /Move up/ }) as HTMLButtonElement).disabled).toBe(true);
    await choose("On the bar (left to right)", "home");
    await fireEvent.click(screen.getByRole("button", { name: "‹ Remove" }));
    expect(shown()).toEqual(["Checking", "Calendar"]);
    expect(available()).toContain("Home");
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(settingsState.navItems).toEqual(["account:1", "tools.calendar"]);
    expect(parseNav(loadPref<string>("navItems", ""))).toEqual(["account:1", "tools.calendar"]);
    expect(dialogState.navbar).toBe(false);
  });

  it("Cancel throws the changes away", async () => {
    render(NavBarModal);
    await choose("On the bar (left to right)", "home");
    await fireEvent.click(screen.getByRole("button", { name: "‹ Remove" }));
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(settingsState.navItems).toEqual(["home", "tools.calendar"]);
    expect(dialogState.navbar).toBe(false);
  });

  it("Reset to default restores the standard bar", async () => {
    render(NavBarModal);
    await fireEvent.click(screen.getByRole("button", { name: "Reset to default" }));
    expect(shown()).toEqual(["Home", "Reminders", "Calendar", "Reconcile (Planned: Phase 5)", "Investments (Planned: Phase 6)"]);
  });

  it("Up and Down need a selection; a deleted account is dropped on Save", async () => {
    settingsState.setNavItems(["home", "account:99"]);
    render(NavBarModal);
    expect(shown()).toEqual(["Home"]);
    expect((screen.getByRole("button", { name: /Move down/ }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(settingsState.navItems).toEqual(["home"]);
  });
});
