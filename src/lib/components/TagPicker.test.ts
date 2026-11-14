import { describe, expect, it, vi, beforeEach } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";

const created = vi.fn();
vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
  return {
    ...real,
    commands: {
      tagCreate: (f: { name: string }) => {
        created(f.name);
        return ok({ id: 9, name: f.name, hidden: false });
      },
    },
  };
});

import TagPicker, { tagsSettled } from "./TagPicker.svelte";
import { listsState } from "../state/lists.svelte";

/** Render the picker, pick New tag…, and type `name`. */
async function typeNew(name: string) {
  const props = { value: "1" };
  render(TagPicker, {
    get value() {
      return props.value;
    },
    set value(v: string) {
      props.value = v;
    },
  });
  const select = screen.getByLabelText("Tag") as HTMLSelectElement;
  select.value = "__new__";
  await fireEvent.change(select);
  const box = await screen.findByLabelText("New tag name");
  await fireEvent.input(box, { target: { value: name } });
  return { box, props };
}

describe("TagPicker New tag… (#67)", () => {
  beforeEach(() => {
    created.mockClear();
    listsState.tags = [
      { id: 1, name: "Old", hidden: false },
      { id: 2, name: "Trip", hidden: false },
    ] as never;
    vi.spyOn(listsState, "loadAll").mockResolvedValue(undefined as never);
  });

  it("leaving the box with a name creates and picks the tag", async () => {
    const { box, props } = await typeNew("X");
    await fireEvent.blur(box);
    await tagsSettled();
    expect(created).toHaveBeenCalledWith("X");
    expect(props.value).toBe("9");
  });

  it("Tab out with a name creates the tag", async () => {
    const { box, props } = await typeNew("X");
    await fireEvent.keyDown(box, { key: "Tab" });
    await fireEvent.blur(box);
    await tagsSettled();
    expect(created).toHaveBeenCalledTimes(1);
    expect(props.value).toBe("9");
  });

  it("a name matching an existing tag in any case picks it", async () => {
    const { box, props } = await typeNew("trip");
    await fireEvent.blur(box);
    await tagsSettled();
    expect(created).not.toHaveBeenCalled();
    expect(props.value).toBe("2");
  });

  it("leaving an empty box goes back to the dropdown", async () => {
    const { box, props } = await typeNew("  ");
    await fireEvent.blur(box);
    await tagsSettled();
    expect(created).not.toHaveBeenCalled();
    expect(props.value).toBe("1");
    expect(screen.getByLabelText("Tag")).toBeTruthy();
  });

  it("Esc cancels and keeps the previous tag", async () => {
    const { box, props } = await typeNew("X");
    await fireEvent.keyDown(box, { key: "Escape" });
    await fireEvent.blur(box);
    await tagsSettled();
    expect(created).not.toHaveBeenCalled();
    expect(props.value).toBe("1");
  });

  it("Enter then blur creates once", async () => {
    const { box, props } = await typeNew("X");
    await fireEvent.keyDown(box, { key: "Enter" });
    await fireEvent.blur(box);
    await tagsSettled();
    expect(created).toHaveBeenCalledTimes(1);
    expect(props.value).toBe("9");
  });
});
