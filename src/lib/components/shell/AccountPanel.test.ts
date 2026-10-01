import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";

const settingsSet = vi.hoisted(() => vi.fn());

vi.mock("../../api", async (orig) => {
  const real = await orig<typeof import("../../api")>();
  return {
    ...real,
    commands: {
      settingsSet: (s: unknown) => (settingsSet(s), Promise.resolve({ status: "ok" as const, data: s })),
    },
  };
});

import AccountPanel from "./AccountPanel.svelte";
import { bookSettings } from "../../state/booksettings.svelte";
import { settingsState } from "../../state/settings.svelte";

beforeEach(() => {
  settingsSet.mockClear();
  bookSettings.reset();
  settingsState.liveWidth = null;
});

describe("AccountPanel width", () => {
  it("is the stock 16rem until dragged", () => {
    expect(settingsState.accountPanelCss).toBe("16rem");
  });

  it("the edge handle shows the live width while dragging and stores it on release", async () => {
    render(AccountPanel);
    const grip = screen.getByRole("separator", { name: "Resize the account list" });
    grip.setPointerCapture = () => {};
    await fireEvent.pointerDown(grip, { clientX: 100, button: 0, pointerId: 1 });
    await fireEvent.pointerMove(grip, { clientX: 160, pointerId: 1 });
    expect(settingsState.liveWidth).not.toBeNull();
    expect(settingsState.accountPanelCss).toBe(`${settingsState.liveWidth}px`);
    await fireEvent.pointerUp(grip, { pointerId: 1 });
    expect(settingsState.liveWidth).toBeNull();
    expect(settingsSet).toHaveBeenCalledOnce();
    expect((settingsSet.mock.calls[0][0] as { account_panel_width: number }).account_panel_width).toBeGreaterThanOrEqual(160);
  });

  it("Home restores the stock width", async () => {
    bookSettings.value = { ...bookSettings.value, account_panel_width: 300 };
    render(AccountPanel);
    expect(settingsState.accountPanelCss).toBe("300px");
    await fireEvent.keyDown(screen.getByRole("separator"), { key: "Home" });
    expect(settingsSet.mock.calls.at(-1)?.[0]).toMatchObject({ account_panel_width: 0 });
  });
});
