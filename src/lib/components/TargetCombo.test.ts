import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return {
    ...real,
    commands: {
      categoryCreatePath: vi.fn(),
      categoryList: vi.fn(),
    },
  };
});

import { commands } from "../api";
import { confirmState } from "../state/confirm.svelte";
import { listsState } from "../state/lists.svelte";
import TargetCombo from "./TargetCombo.svelte";

const c = vi.mocked(commands, true);
const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
const cat = (id: number, name: string, parent: number | null = null) => ({
  id, parent, kind: "expense" as const, name, system: null, tax_related: false,
  tithable: false, giving: false, hidden: false, created_at: "",
});

const box = () => screen.getByLabelText("Category") as HTMLInputElement;
const listed = () =>
  within(screen.getByRole("listbox")).getAllByRole("option").map((o) => o.textContent?.trim());

beforeEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.clearAllMocks();
  listsState.categories = [cat(5, "Food")];
  listsState.accounts = [];
  c.categoryList.mockImplementation(() => ok([cat(5, "Food"), cat(7, "Fuel")]));
  c.categoryCreatePath.mockImplementation((path: string) => ok(cat(7, path)));
});

describe("TargetCombo: entering a new category", () => {
  it("offers to create text that matches nothing, and creates it after a yes", async () => {
    const ask = vi.spyOn(confirmState, "ask").mockResolvedValue(true);
    render(TargetCombo, { value: "", newKind: "expense" });
    await fireEvent.input(box(), { target: { value: "Fuel" } });
    expect(listed()).toEqual(["+ Create new expense category “Fuel”"]);
    await fireEvent.keyDown(box(), { key: "Enter" });
    await waitFor(() => expect(c.categoryCreatePath).toHaveBeenCalledWith("Fuel", "expense"));
    expect(ask).toHaveBeenCalledWith('Create new expense category "Fuel"?');
    await waitFor(() => expect(box().value).toBe("Fuel"));
  });

  it("passes a Parent:Child path through, with the income kind for a deposit", async () => {
    vi.spyOn(confirmState, "ask").mockResolvedValue(true);
    render(TargetCombo, { value: "", newKind: "income" });
    await fireEvent.input(box(), { target: { value: "Charity: Fast Offering" } });
    await fireEvent.keyDown(box(), { key: "Tab" });
    await waitFor(() =>
      expect(c.categoryCreatePath).toHaveBeenCalledWith("Charity:Fast Offering", "income"),
    );
  });

  it("creates nothing when the confirmation is declined", async () => {
    vi.spyOn(confirmState, "ask").mockResolvedValue(false);
    render(TargetCombo, { value: "", newKind: "expense" });
    await fireEvent.input(box(), { target: { value: "Fuel" } });
    await fireEvent.keyDown(box(), { key: "Enter" });
    await waitFor(() => expect(box().value).toBe(""));
    expect(c.categoryCreatePath).not.toHaveBeenCalled();
  });

  it("keeps the best match as the default; creating is a separate row", async () => {
    render(TargetCombo, { value: "", newKind: "expense" });
    await fireEvent.input(box(), { target: { value: "foo" } });
    expect(listed()).toEqual(["Food", "+ Create new expense category “foo”"]);
    await fireEvent.input(box(), { target: { value: "Food" } });
    expect(listed()).toEqual(["Food"]); // exact match: nothing to create
    await fireEvent.keyDown(box(), { key: "Enter" });
    expect(box().value).toBe("Food");
    expect(c.categoryCreatePath).not.toHaveBeenCalled();
  });

  it("shows the reason when Rust refuses", async () => {
    vi.spyOn(confirmState, "ask").mockResolvedValue(true);
    c.categoryCreatePath.mockImplementation(() =>
      Promise.resolve({ status: "error" as const, error: { kind: "invalid", message: "no equity here" } as never }),
    );
    render(TargetCombo, { value: "", newKind: "expense" });
    await fireEvent.input(box(), { target: { value: "Fuel" } });
    await fireEvent.keyDown(box(), { key: "Enter" });
    expect((await screen.findByRole("alert")).textContent).toContain("Could not create it");
  });

  it("offers nothing to create without newKind", async () => {
    render(TargetCombo, { value: "" });
    await fireEvent.input(box(), { target: { value: "Fuel" } });
    expect(listed()).toEqual(["No match"]);
  });
});
