// Themes (SET-010) and the one base font size (SET-020). Colors and the
// font family live only in src/css/themes/*.css; components size text
// only with the named sizes in src/css/base.css. Reads the sources, since
// the test DOM does not apply stylesheets.

import { describe, expect, it } from "vitest";
import { THEMES } from "./state/theme.svelte";

const raw = (glob: Record<string, string>) =>
  Object.fromEntries(Object.entries(glob).map(([path, src]) => [path.slice("/src/".length), src]));

const COMPONENTS = raw(import.meta.glob<string>("/src/**/*.svelte", { query: "?raw", import: "default", eager: true }));
const THEME_FILES = raw(import.meta.glob<string>("/src/css/themes/*.css", { query: "?raw", import: "default", eager: true }));
const BASE = raw(import.meta.glob<string>("/src/css/base.css", { query: "?raw", import: "default", eager: true }))[
  "css/base.css"
];

const noComments = (css: string) => css.replace(/\/\*[\s\S]*?\*\//g, "");

/** A component's <style> contents, or "". */
function style(src: string): string {
  const at = src.indexOf("<style>");
  return at < 0 ? "" : src.slice(at + "<style>".length, src.lastIndexOf("</style>"));
}

/** CSS with every `@media print { … }` block removed: print keeps black
 * on white and point sizes. */
function screenOnly(css: string): string {
  let out = css;
  for (let at = out.indexOf("@media print"); at >= 0; at = out.indexOf("@media print")) {
    let i = out.indexOf("{", at);
    let depth = 0;
    for (; i < out.length; i++) {
      if (out[i] === "{") depth++;
      else if (out[i] === "}" && --depth === 0) break;
    }
    out = out.slice(0, at) + out.slice(i + 1);
  }
  return out;
}

/** Custom properties a stylesheet (or markup) sets. */
const defined = (css: string) => new Set([...noComments(css).matchAll(/(--[a-z0-9-]+)\s*:/g)].map((m) => m[1]));
/** Custom properties a stylesheet reads. */
const used = (css: string) => new Set([...noComments(css).matchAll(/var\((--[a-z0-9-]+)/g)].map((m) => m[1]));

const themeVars = Object.fromEntries(Object.entries(THEME_FILES).map(([f, css]) => [f, defined(css)]));

describe("theme files", () => {
  it("there is one file per theme in Settings", () => {
    expect(Object.keys(THEME_FILES).sort()).toEqual(THEMES.map((t) => `css/themes/${t.value}.css`).sort());
  });

  it("each is scoped to its own data-theme", () => {
    for (const t of THEMES) {
      expect(THEME_FILES[`css/themes/${t.value}.css`]).toMatch(new RegExp(`\\[data-theme="${t.value}"\\]\\s*\\{`));
    }
  });

  it("every theme defines the same variables", () => {
    const [first, ...rest] = Object.entries(themeVars);
    for (const [file, vars] of rest) {
      expect({ file, vars: [...vars].sort() }).toEqual({ file, vars: [...first[1]].sort() });
    }
  });

  it("themes set no sizes: the base size is a setting", () => {
    for (const [file, css] of Object.entries(THEME_FILES)) {
      expect({ file, sizes: noComments(css).match(/font-size|--fs-/g) }).toEqual({ file, sizes: null });
    }
  });

  it("every variable read is defined by a theme, base.css, or the component itself", () => {
    const themed = Object.values(themeVars)[0];
    const base = defined(BASE);
    const local = new Set(Object.values(COMPONENTS).flatMap((src) => [...defined(src)]));
    const missing: string[] = [];
    for (const [file, css] of [["css/base.css", BASE], ...Object.entries(COMPONENTS).map(([f, s]) => [f, style(s)])]) {
      for (const v of used(css)) {
        if (!themed.has(v) && !base.has(v) && !local.has(v)) missing.push(`${file}: ${v}`);
      }
    }
    expect(missing).toEqual([]);
  });
});

/** WCAG contrast of two `#rrggbb` colors. */
function contrast(a: string, b: string): number {
  const lum = (hex: string) => {
    const [r, g, bl] = [1, 3, 5].map((i) => {
      const c = parseInt(hex.slice(i, i + 2), 16) / 255;
      return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    });
    return 0.2126 * r + 0.7152 * g + 0.0722 * bl;
  };
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

describe("fonts", () => {
  it("no theme sets one: the font is a setting", () => {
    for (const [file, css] of Object.entries(THEME_FILES)) {
      expect({ file, fonts: noComments(css).match(/font-family|--font-ui/g) }).toEqual({ file, fonts: null });
    }
  });
});

describe("Matrix theme", () => {
  const css = noComments(THEME_FILES["css/themes/matrix.css"]);
  const value = (name: string) => css.match(new RegExp(`${name}:\\s*([^;]+);`))?.[1].trim() ?? "";

  it("keeps text at 4.5:1 or better on the solid backgrounds it sits on", () => {
    const pairs: [string, string][] = [
      ["--fg", "--bg"], ["--fg", "--panel-bg"], ["--menubar-fg", "--menubar-bg"], ["--nav-fg", "--nav-bg"],
      ["--nav-btn-fg", "--nav-btn-bg"], ["--title-fg", "--title-bg"], ["--tabs-fg", "--tabs-bg"],
      ["--btn-fg", "--btn-bg"], ["--opt-fg", "--opt-bg"], ["--head-fg", "--head-bg"],
      ["--head-sorted-fg", "--head-sorted-bg"], ["--focus-fg", "--focus-bg"], ["--focus-fg", "--focus-sel-bg"],
      ["--sel-fg", "--sel-bg"], ["--reconciled-fg", "--row-bg"], ["--bad", "--bg"], ["--good", "--bg"],
      ["--nav-badge", "--nav-bg"], ["--fg", "--filter-bg"], ["--fg", "--popup-bg"],
    ];
    const low = pairs
      .map(([fg, bg]) => ({ fg, bg, ratio: contrast(value(fg), value(bg)) }))
      .filter((p) => !(p.ratio >= 4.5));
    expect(low).toEqual([]);
  });

  it("OK and problem colors differ from the green text and from each other", () => {
    const [fg, good, bad] = [value("--fg"), value("--good"), value("--bad")];
    expect(new Set([fg, good, bad]).size).toBe(3);
  });
});

describe("Nordic theme", () => {
  const nordic = noComments(THEME_FILES["css/themes/nordic.css"]);
  const value = (css: string, name: string) => css.match(new RegExp(`${name}:\\s*([^;]+);`))?.[1].trim() ?? "";

  it("has a filled navigation bar, unlike Nordic's own transparent one", () => {
    expect(value(nordic, "--nav-bg")).toMatch(/^#[0-9a-f]{6}$/);
  });

  it("keeps text at 4.5:1 or better on the solid backgrounds it sits on", () => {
    const v = (n: string) => value(nordic, n);
    const pairs: [string, string][] = [
      ["--fg", "--bg"], ["--fg", "--panel-bg"], ["--menubar-fg", "--menubar-bg"], ["--nav-fg", "--nav-bg"],
      ["--nav-btn-fg", "--nav-btn-hover-bg"], ["--title-fg", "--title-bg"], ["--tabs-fg", "--tabs-bg"],
      ["--btn-fg", "--btn-bg"], ["--btn-fg", "--btn-hover-bg"], ["--opt-fg", "--opt-bg"],
      ["--head-fg", "--head-bg"], ["--head-sorted-fg", "--head-sorted-bg"], ["--focus-fg", "--focus-bg"],
      ["--focus-fg", "--focus-sel-bg"], ["--sel-fg", "--sel-bg"], ["--reconciled-fg", "--row-bg"],
      ["--bad", "--bg"], ["--good", "--bg"], ["--bad", "--row-bg"], ["--good", "--row-bg"],
      ["--nav-badge", "--nav-bg"], ["--fg", "--filter-bg"], ["--fg", "--popup-bg"],
    ];
    const low = pairs
      .map(([fg, bg]) => ({ fg, bg, ratio: contrast(v(fg), v(bg)) }))
      .filter((p) => !(p.ratio >= 4.5));
    expect(low).toEqual([]);
  });

  it("OK, problem, and text colors are three different colors", () => {
    expect(new Set([value(nordic, "--fg"), value(nordic, "--good"), value(nordic, "--bad")]).size).toBe(3);
  });
});

describe("components", () => {
  it("hold no color literals: colors come from the theme", () => {
    const bad: string[] = [];
    const literal = /#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?)\(|:\s*(?:white|black|red|green|blue|gray|grey|orange|yellow)\b/;
    for (const [file, src] of [...Object.entries(COMPONENTS).map(([f, s]) => [f, style(s)]), ["css/base.css", BASE]]) {
      for (const line of screenOnly(noComments(src)).split("\n")) {
        if (literal.test(line)) bad.push(`${file}: ${line.trim()}`);
      }
    }
    expect(bad).toEqual([]);
  });

  it("size text only with the named sizes (print may use pt)", () => {
    const bad: string[] = [];
    for (const [file, src] of Object.entries(COMPONENTS)) {
      for (const m of screenOnly(noComments(style(src))).matchAll(/font-size:\s*([^;]+);/g)) {
        if (!/^var\(--fs-[a-z]+\)$/.test(m[1].trim())) bad.push(`${file}: ${m[1]}`);
      }
    }
    expect(bad).toEqual([]);
  });

  it("every named size is in rem, so it follows the base size", () => {
    const sizes = [...noComments(BASE).matchAll(/(--fs-[a-z]+):\s*([^;]+);/g)];
    expect(sizes.length).toBeGreaterThan(0);
    for (const [, name, value] of sizes) expect({ name, value }).toEqual({ name, value: expect.stringMatching(/^[\d.]+rem$/) });
  });

  it("every named size a component uses exists", () => {
    const names = defined(BASE);
    const bad = Object.entries(COMPONENTS).flatMap(([file, src]) =>
      [...used(style(src))].filter((v) => v.startsWith("--fs-") && !names.has(v)).map((v) => `${file}: ${v}`),
    );
    expect(bad).toEqual([]);
  });
});
