import { beforeEach, describe, expect, it } from "vitest";
import { viewState } from "./view.svelte";

beforeEach(() => viewState.reset());

describe("view history", () => {
  it("navigate records where you have been; back and forward walk it", () => {
    viewState.navigate("search");
    viewState.navigate("manage", { tab: "tags" });
    expect(viewState.current).toBe("manage");
    expect(viewState.params).toEqual({ tab: "tags" });
    viewState.back();
    expect(viewState.current).toBe("search");
    viewState.back();
    expect(viewState.current).toBe("insights");
    expect(viewState.canBack).toBe(false);
    viewState.forward();
    viewState.forward();
    expect(viewState.current).toBe("manage");
    expect(viewState.canForward).toBe(false);
  });

  it("going where you already are adds nothing", () => {
    viewState.navigate("search");
    viewState.navigate("search");
    viewState.back();
    expect(viewState.current).toBe("insights");
    // A different tab of the same view is a different place.
    viewState.navigate("manage", { tab: "payees" });
    viewState.navigate("manage", { tab: "tags" });
    viewState.back();
    expect(viewState.params).toEqual({ tab: "payees" });
  });

  it("going somewhere new drops the forward entries", () => {
    viewState.navigate("search");
    viewState.navigate("manage");
    viewState.back();
    viewState.navigate("insights");
    expect(viewState.canForward).toBe(false);
    viewState.back();
    expect(viewState.current).toBe("search");
  });

  it("untouched until the first navigation", () => {
    expect(viewState.untouched).toBe(true);
    viewState.navigate("search");
    expect(viewState.untouched).toBe(false);
  });
});
