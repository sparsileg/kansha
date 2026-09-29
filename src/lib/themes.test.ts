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
