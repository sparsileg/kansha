import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import FilterBar from "./FilterBar.svelte";

describe("FilterBar", () => {
  it("has the field filters but no text search (that is the navigation bar's)", () => {
    render(FilterBar);
    for (const l of ["From", "To", "Payee", "Category", "Tag", "Cleared"]) {
      expect(screen.getByLabelText(l)).toBeTruthy();
    }
    expect(screen.queryByRole("searchbox")).toBeNull();
    expect(screen.getByRole("button", { name: "Clear all" })).toBeTruthy();
  });
});
