import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import FilterBar from "./FilterBar.svelte";
import { listsState } from "../state/lists.svelte";

describe("FilterBar", () => {
  it("has the field filters but no text search (that is the navigation bar's)", () => {
    render(FilterBar);
    for (const l of ["From", "To", "Payee", "Category", "Tag", "Cleared"]) {
      expect(screen.getByLabelText(l)).toBeTruthy();
    }
    expect(screen.queryByRole("searchbox")).toBeNull();
    expect(screen.getByRole("button", { name: "Clear all" })).toBeTruthy();
  });

  it("a short From / To date gets its year when changed; bad text is kept", async () => {
    listsState.today = "2026-09-24";
    render(FilterBar);
    const from = screen.getByLabelText("From") as HTMLInputElement;
    const to = screen.getByLabelText("To") as HTMLInputElement;
    await fireEvent.input(from, { target: { value: "3/5" } });
    await fireEvent.change(from);
    expect(from.value).toBe("03/05/2026");
    await fireEvent.input(to, { target: { value: "abc" } });
    await fireEvent.change(to);
    expect(to.value).toBe("abc");
  });
});
