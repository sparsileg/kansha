import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return { ...real, commands: { appVersion: () => Promise.resolve("0.7.0") } };
});

import AboutModal from "./AboutModal.svelte";
import { dialogState } from "../state/dialogs.svelte";
import { runAction } from "../shell/actions";
import { MENUS } from "../shell/menus";

beforeEach(() => {
  cleanup();
  dialogState.about = true;
});

describe("Help > About Kansha", () => {
  it("shows the version", async () => {
    render(AboutModal);
    expect(await screen.findByText("Version 0.7.0")).toBeTruthy();
  });

  it("Close closes it", async () => {
    render(AboutModal);
    await fireEvent.click(screen.getByText("Close"));
    expect(dialogState.about).toBe(false);
  });

  it("the menu item is enabled and opens it", () => {
    const item = MENUS.find((m) => m.id === "help")?.items.find((i) => "id" in i && i.id === "help.about");
    expect(item).toBeTruthy();
    expect((item as { disabled?: string }).disabled).toBeUndefined();
    dialogState.about = false;
    runAction("help.about");
    expect(dialogState.about).toBe(true);
  });
});
