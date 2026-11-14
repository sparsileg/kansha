import { describe, expect, it, vi } from "vitest";

const sent = vi.hoisted(() => vi.fn());
vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return { ...real, commands: { popupColors: (c: unknown) => (sent(c), Promise.resolve({ status: "ok", data: null })) } };
});

import { popupColorsOf, syncPopupColors } from "./popupColors";

function themed(): HTMLElement {
  const root = document.createElement("div");
  root.style.setProperty("--opt-bg", " #000000");
  root.style.setProperty("--opt-fg", "#00ff00");
  root.style.setProperty("--sel-bg", "#00ff00");
  root.style.setProperty("--sel-fg", "#000000");
  document.body.append(root);
  return root;
}

describe("open dropdown colors (NFR-080, #18)", () => {
  it("reads the theme's option and selection colors and the font size", () => {
    expect(popupColorsOf(themed(), 20.4)).toEqual({
      bg: "#000000",
      fg: "#00ff00",
      sel_bg: "#00ff00",
      sel_fg: "#000000",
      font_px: 20,
    });
  });

  it("sends nothing before the theme's CSS is in", async () => {
    sent.mockClear();
    await syncPopupColors(document.createElement("div"), 16);
    expect(sent).not.toHaveBeenCalled();
    await syncPopupColors(themed(), 16);
    expect(sent).toHaveBeenCalledTimes(1);
  });
});
