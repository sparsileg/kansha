import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return {
    ...real,
    commands: { appVersion: () => Promise.resolve("0.8.0"), schemaVersion: () => Promise.resolve(10) },
  };
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
  it("shows the app and schema versions", async () => {
    render(AboutModal);
    expect(await screen.findByText("0.8.0")).toBeTruthy();
    expect(await screen.findByText("10")).toBeTruthy();
    expect(screen.getByText("Version")).toBeTruthy();
    expect(screen.getByText("Schema")).toBeTruthy();
  });

  it("describes Kansha and gives the address, as plain text", () => {
    render(AboutModal);
    expect(screen.getByText(/personal finance app/)).toBeTruthy();
    expect(screen.getByText(/Japanese for gratitude\./)).toBeTruthy();
    expect(screen.queryByText(/heartfelt thanks/)).toBeNull();
    const email = screen.getByText("feedback@astryx.tools");
    expect(email.closest("a")).toBeNull();
  });

  it("shows the copyright and license, and the full text on request", async () => {
    render(AboutModal);
    expect(screen.getByText(/© 2026 sparsile\. MIT License\./)).toBeTruthy();
    expect(screen.queryByText(/Permission is hereby granted/)).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "License" }));
    expect(screen.getByText(/Permission is hereby granted/)).toBeTruthy();
    // Text is whitespace-normalized: the next line follows a space.
    expect(screen.getByText(/Copyright \(c\) 2026 sparsile Permission/)).toBeTruthy();
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
