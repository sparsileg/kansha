import { afterEach, describe, expect, it, vi } from "vitest";
import { drawer, SLIDE_MS } from "./motion";

const node = (width: number) => ({ offsetWidth: width }) as HTMLElement;

afterEach(() => vi.unstubAllGlobals());

describe("drawer", () => {
  it("slides the panel from nothing to its full width", () => {
    const t = drawer(node(240));
    expect(t.duration).toBe(SLIDE_MS);
    expect(t.css(0)).toContain("width: 0px");
    expect(t.css(0.5)).toContain("width: 120px");
    expect(t.css(1)).toContain("width: 240px");
    expect(t.css(0.5)).toContain("overflow: hidden");
  });

  it("is instant for those who ask for less motion", () => {
    vi.stubGlobal("matchMedia", (q: string) => ({ matches: q.includes("reduce") }));
    expect(drawer(node(240)).duration).toBe(0);
  });
});
