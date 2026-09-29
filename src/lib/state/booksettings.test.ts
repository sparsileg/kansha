import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "../types/bindings";

const store = vi.hoisted(() => ({
  saved: null as unknown,
  appearance: null as unknown,
  stored: { theme: "classic", font: "arial", font_size: 18 } as unknown,
  fail: false,
}));

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });
  return {
    ...real,
    commands: {
      settingsGet: () => ok({ ...(store.saved as object), date_format: "dmy", startup: "calendar" }),
      settingsSet: (s: Settings) => {
        store.saved = s;
        return store.fail
          ? Promise.resolve({ status: "error" as const, error: { kind: "invalid" as const, message: "no" } })
          : ok(s);
      },
      appearanceGet: () => Promise.resolve(store.stored),
      appearanceSet: (a: unknown) => {
        store.appearance = a;
        return Promise.resolve({ status: "ok" as const, data: null });
      },
    },
  };
});

import { bookSettings, DEFAULT_SETTINGS } from "./booksettings.svelte";
import { dateFormatState } from "./dateformat.svelte";
import { settingsState } from "./settings.svelte";
import { themeState } from "./theme.svelte";

beforeEach(() => {
  bookSettings.reset();
  store.saved = { ...DEFAULT_SETTINGS };
  store.stored = { theme: "classic", font: "arial", font_size: 18 };
  store.fail = false;
  localStorage.clear();
});

describe("book settings (SET-070)", () => {
  it("load from the book and feed the setting states", async () => {
    expect(dateFormatState.value).toBe("mdy");
    await bookSettings.load();
    expect(dateFormatState.value).toBe("dmy");
    expect(settingsState.startup).toBe("calendar");
  });

  it("changes are stored in the book, not in localStorage", async () => {
    settingsState.setAccountPanelSide("right");
    settingsState.setNavItems(["home"]);
    dateFormatState.set("ymd");
    await vi.waitFor(() => expect((store.saved as Settings).date_format).toBe("ymd"));
    const saved = store.saved as Settings;
    expect(saved.account_panel_side).toBe("right");
    expect(saved.nav_items).toBe('["home"]');
    expect(settingsState.navItems).toEqual(["home"]);
    expect(localStorage.length).toBe(0);
  });

  it("a refused change still applies for the session and reports the error", async () => {
    store.fail = true;
    const err = await bookSettings.update({ upcoming_days: 999 });
    expect(err).toBe("no");
    expect(bookSettings.value.upcoming_days).toBe(999);
  });
});

describe("theme, font, and font size (per computer)", () => {
  it("load from the config file and save changes to it", async () => {
    await themeState.load();
    expect(themeState.theme).toBe("classic");
    expect(themeState.font).toBe("arial");
    expect(themeState.fontSize).toBe(18);
    themeState.setFontSize(20);
    await vi.waitFor(() => expect(store.appearance).toEqual({ theme: "classic", font: "arial", font_size: 20 }));
    themeState.setFont("courier");
    await vi.waitFor(() => expect(store.appearance).toEqual({ theme: "classic", font: "courier", font_size: 20 }));
    expect(localStorage.length).toBe(0);
  });

  it("a config from before fonts keeps the System font", async () => {
    store.stored = { theme: "matrix", font: null, font_size: 16 };
    themeState.setFont("system");
    await themeState.load();
    expect(themeState.theme).toBe("matrix");
    expect(themeState.font).toBe("system");
  });

  it("a stored Nordic Courier becomes Nordic with the Courier New font", async () => {
    store.stored = { theme: "nordic-courier", font: null, font_size: 14 };
    themeState.setFont("system");
    await themeState.load();
    expect(themeState.theme).toBe("nordic");
    expect(themeState.font).toBe("courier");
  });

  it("an unknown stored font is ignored", async () => {
    store.stored = { theme: "dark", font: "comic-sans", font_size: 14 };
    themeState.setFont("verdana");
    await themeState.load();
    expect(themeState.font).toBe("verdana");
  });
});
