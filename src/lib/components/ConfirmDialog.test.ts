import { beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";

import ConfirmDialog from "./ConfirmDialog.svelte";
import { confirmState } from "../state/confirm.svelte";

beforeEach(() => {
  cleanup();
});

describe("Confirm dialog", () => {
  it("sits above the dialog that asked, and OK answers it", async () => {
    render(ConfirmDialog);
    const answer = confirmState.ask("Delete this investment transaction?");
    const text = await screen.findByText("Delete this investment transaction?");
    expect(text.closest(".backdrop")?.classList.contains("top")).toBe(true);
    await fireEvent.click(screen.getByText("OK"));
    expect(await answer).toBe(true);
  });
});
