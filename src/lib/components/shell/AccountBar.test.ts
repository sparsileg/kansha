import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";

const open = vi.hoisted(() => vi.fn());
vi.mock("../../state/register.svelte", async () => {
  const { registerStub } = await import("./registerStub");
  return { registerState: registerStub(open) };
});

import AccountBar from "./AccountBar.svelte";
import { listsState } from "../../state/lists.svelte";
import { registerState } from "../../state/register.svelte";
import { bookSettings } from "../../state/booksettings.svelte";
import { settingsState } from "../../state/settings.svelte";
import { statusState } from "../../state/status.svelte";
import { viewState } from "../../state/view.svelte";

const acct = (id: number, name: string, status = "open") =>
  ({ id, name, account_type: "checking", group: "banking", status, show_in_list: true, sort_order: id, investment: null }) as never;

beforeEach(() => {
  vi.clearAllMocks();
  listsState.accounts = [acct(1, "Checking"), acct(2, "Old", "closed")];
  listsState.balances = [];
  settingsState.showClosedAccounts = false;
  settingsState.setAccountPanelOpen(true);
  settingsState.setAccountPanelSide("left");
  registerState.accountId = null;
  viewState.reset();
  statusState.clear();
});

describe("AccountBar", () => {
  it("open: the Accounts button closes the panel, and the choice is kept", async () => {
    render(AccountBar);
    const head = screen.getByRole("button", { name: "Accounts" });
    expect(head.getAttribute("aria-expanded")).toBe("true");
    await fireEvent.click(head);
    expect(settingsState.accountPanelOpen).toBe(false);
    expect(bookSettings.value.account_panel_open).toBe(false);
    expect(screen.getByRole("button", { name: "Accounts" }).getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("closed: Accounts drops the list down; picking one opens it and closes the drop-down", async () => {
    settingsState.setAccountPanelOpen(false);
    render(AccountBar);
    await fireEvent.click(screen.getByRole("button", { name: "Accounts" }));
    expect(screen.getByRole("menu", { name: "Accounts" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Old/ })).toBeNull(); // closed accounts hidden
    await fireEvent.click(screen.getByLabelText("Show closed accounts"));
    expect(screen.getByRole("button", { name: /Old \(closed\)/ })).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: /Checking/ }));
    expect(viewState.current).toBe("account");
    expect(open).toHaveBeenCalledWith(1);
    expect(screen.queryByRole("menu")).toBeNull();
    expect(settingsState.accountPanelOpen).toBe(false); // still closed
  });

  it("the drop-down can bring the panel back", async () => {
    settingsState.setAccountPanelOpen(false);
    render(AccountBar);
    await fireEvent.click(screen.getByRole("button", { name: "Accounts" }));
    await fireEvent.click(screen.getByRole("button", { name: "Keep open" }));
    expect(settingsState.accountPanelOpen).toBe(true);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("Esc and an outside click close the drop-down", async () => {
    settingsState.setAccountPanelOpen(false);
    render(AccountBar);
    const head = screen.getByRole("button", { name: "Accounts" });
    await fireEvent.click(head);
    await fireEvent.keyDown(head, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    await fireEvent.click(head);
    await fireEvent.pointerDown(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("shows the status message, and clears it when it expires", async () => {
    render(AccountBar);
    const bar = screen.getByRole("status");
    expect(bar.textContent).toBe("");
    statusState.show("Integrity check found no problems with the data.");
    expect(await screen.findByText("Integrity check found no problems with the data.")).toBeTruthy();
    expect(bar.classList.contains("alert")).toBe(false);
    statusState.clear();
    await vi.waitFor(() => expect(bar.textContent).toBe(""));
  });

  it("an alert is marked to flash", async () => {
    render(AccountBar);
    statusState.show("The backup failed.", "alert");
    await screen.findByText("The backup failed.");
    expect(screen.getByRole("status").classList.contains("alert")).toBe(true);
  });

  it("the message sits opposite the Accounts button", () => {
    const { container } = render(AccountBar);
    const bar = container.querySelector(".bar")!;
    expect(bar.classList.contains("right")).toBe(false);
    settingsState.setAccountPanelSide("right");
    return vi.waitFor(() => expect(container.querySelector(".bar")!.classList.contains("right")).toBe(true));
  });

  it("an empty book has no Accounts button but still has the status bar", async () => {
    listsState.accounts = [];
    listsState.loaded = true;
    render(AccountBar);
    expect(screen.queryByRole("button", { name: "Accounts" })).toBeNull();
    statusState.show("Hello");
    expect(await screen.findByText("Hello")).toBeTruthy();
    listsState.loaded = false;
  });
});
