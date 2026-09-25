import { beforeEach, describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import NavBar from "./NavBar.svelte";
import { listsState } from "../../state/lists.svelte";
import { registerState } from "../../state/register.svelte";
import { scheduleState } from "../../state/schedule.svelte";
import { settingsState } from "../../state/settings.svelte";
import { viewState } from "../../state/view.svelte";

beforeEach(() => {
  viewState.reset();
  settingsState.setHome("calendar");
  settingsState.setNavItems(["home", "tools.reminders", "tools.calendar", "tools.reconcile", "view.investments"]);
  scheduleState.due = [];
  scheduleState.review = [];
});

describe("NavBar", () => {
  it("Reminders shows the count that needs attention, and only then", async () => {
    render(NavBar);
    expect(screen.getByRole("button", { name: "Reminders" })).toBeTruthy();
    scheduleState.due = [{}, {}] as never;
    scheduleState.review = [{}] as never;
    expect(await screen.findByRole("button", { name: "Reminders 3" })).toBeTruthy();
  });

  it("quick jumps navigate; Home goes to the home screen setting", async () => {
    render(NavBar);
    await fireEvent.click(screen.getByRole("button", { name: "Reminders" }));
    expect(viewState.current).toBe("scheduled");
    await fireEvent.click(screen.getByRole("button", { name: "Home" }));
    await Promise.resolve();
    expect(viewState.current).toBe("calendar");
  });

  it("planned buttons are greyed with a reason and do nothing", async () => {
    settingsState.setNavItems(["file.backup", "view.investments"]);
    render(NavBar);
    const backup = screen.getByRole("button", { name: "Backup" });
    expect(backup.getAttribute("aria-disabled")).toBe("true");
    expect(backup.getAttribute("title")).toBe("Planned: Phase 8");
    await fireEvent.click(backup);
    expect(viewState.current).toBe("dashboard");
    const inv = screen.getByRole("button", { name: "Investments" });
    expect(inv.getAttribute("aria-disabled")).toBeNull();
    await fireEvent.click(inv);
    expect(viewState.current).toBe("investments");
  });
});

describe("NavBar contents come from the setting", () => {
  it("shows the chosen items in the chosen order, including an account", () => {
    listsState.accounts = [{ id: 4, name: "Checking" }] as never;
    settingsState.setNavItems(["account:4", "tools.calendar", "tools.payees"]);
    render(NavBar);
    const names = screen.getAllByRole("button").map((b) => b.textContent?.trim());
    expect(names).toEqual(["Checking", "Calendar", "Payees"]);
  });

  it("clicking an account item opens that account", async () => {
    listsState.accounts = [{ id: 4, name: "Checking" }] as never;
    settingsState.setNavItems(["account:4"]);
    registerState.accountId = 4; // already open: just navigates back to it
    render(NavBar);
    await fireEvent.click(screen.getByRole("button", { name: "Checking" }));
    await Promise.resolve();
    expect(viewState.current).toBe("account");
  });

  it("marks the current view's button", async () => {
    render(NavBar);
    await fireEvent.click(screen.getByRole("button", { name: "Calendar" }));
    expect(screen.getByRole("button", { name: "Calendar" }).getAttribute("aria-current")).toBe("page");
    expect(screen.getByRole("button", { name: "Home" }).getAttribute("aria-current")).toBeNull();
  });
});

describe("NavBar search", () => {
  const box = () => screen.getByRole("searchbox", { name: "Search" }) as HTMLInputElement;

  it("Enter shows the Search view with the text; an empty box does nothing", async () => {
    render(NavBar);
    await fireEvent.submit(box().closest("form")!);
    expect(viewState.current).toBe("dashboard");
    await fireEvent.input(box(), { target: { value: "  costco " } });
    await fireEvent.submit(box().closest("form")!);
    expect(viewState.current).toBe("search");
    expect(viewState.params).toEqual({ q: "costco" });
  });

  it("offers 'This account' only inside an account, and then limits the search to it", async () => {
    listsState.accounts = [{ id: 4, name: "Checking" }] as never;
    registerState.accountId = 4;
    render(NavBar);
    expect(screen.queryByLabelText("This account")).toBeNull();
    viewState.navigate("account");
    await screen.findByLabelText("This account");
    await fireEvent.input(box(), { target: { value: "rent" } });
    await fireEvent.submit(box().closest("form")!);
    expect(viewState.params).toEqual({ q: "rent" }); // unchecked: everywhere
    viewState.navigate("account");
    await fireEvent.click(await screen.findByLabelText("This account"));
    await fireEvent.submit(box().closest("form")!);
    expect(viewState.params).toEqual({ q: "rent", account: 4 });
  });

  it("shows the text of the search being viewed", async () => {
    render(NavBar);
    viewState.navigate("search", { q: "gas" });
    await Promise.resolve();
    await Promise.resolve();
    expect(box().value).toBe("gas");
  });
});
