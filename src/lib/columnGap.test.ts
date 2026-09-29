// Every table keeps at least --col-gap (6px) between its columns: a
// right-aligned amount must never touch the text in the next column.
// Reads the style blocks of every component, since the test DOM does not
// lay anything out.

import { describe, expect, it } from "vitest";

const MIN_PX = 6;

// Every component's source, by path from src/.
const SOURCES = Object.fromEntries(
  Object.entries(import.meta.glob<string>("/src/**/*.svelte", { query: "?raw", import: "default", eager: true })).map(
    ([path, src]) => [path.slice("/src/".length), src],
  ),
);

/** Each CSS rule in a component's <style>: selector and body. */
function rules(file: string, src: string): { file: string; selector: string; body: string }[] {
  if (!src.includes("<style>")) return [];
  const style = src.slice(src.indexOf("<style>") + "<style>".length, src.lastIndexOf("</style>"));
  const out: { file: string; selector: string; body: string }[] = [];
  for (const m of style.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    out.push({ file, selector: m[1].trim(), body: m[2] });
  }
  return out;
}

/** A CSS length in px, or null if it is not a plain length. The gap
 * variable counts as the minimum. */
function px(v: string): number | null {
  if (v.startsWith("var(--col-gap")) return MIN_PX;
  const m = /^(-?[\d.]+)(px|rem|em)?$/.exec(v);
  if (!m) return null;
  return Number(m[1]) * (m[2] === "rem" || m[2] === "em" ? 16 : 1);
}

function decl(body: string, prop: string): string | null {
  const m = new RegExp(`(?:^|;|\\s)${prop}\\s*:\\s*([^;]+)`).exec(body);
  return m ? m[1].trim() : null;
}

/** Split a value on spaces outside parentheses. */
const parts = (v: string) => v.match(/(?:[^\s(]+|\([^)]*\))+/g) ?? [];

const all = Object.entries(SOURCES).flatMap(([file, src]) => rules(file, src));

// Grids that are not tables: a month of day cells drawn with borders, and
// a rule that only changes the columns of a grid set elsewhere.
const NOT_TABLES = new Set([
  "views/Calendar.svelte .grid",
  "views/Calendar.svelte .layout",
  "lib/components/invest/DatePicker.svelte .grid",
  "lib/components/ScheduleModal.svelte .line.one",
]);

describe("space between columns", () => {
  it("finds the components' rules", () => {
    expect(all.some((r) => r.file.endsWith("RegisterGrid.svelte") && r.body.includes("grid-template-columns"))).toBe(true);
  });

  it("every grid with columns has a column gap of at least 6px", () => {
    const bad: string[] = [];
    for (const r of all) {
      if (!r.body.includes("grid-template-columns") || NOT_TABLES.has(`${r.file} ${r.selector}`)) continue;
      const colGap = decl(r.body, "column-gap") ?? parts(decl(r.body, "gap") ?? "").at(-1) ?? null;
      const n = colGap === null ? null : px(colGap);
      if (n === null || n < MIN_PX) bad.push(`${r.file} ${r.selector}: ${colGap ?? "no gap"}`);
    }
    expect(bad).toEqual([]);
  });

  it("no table cell pads its sides below half the gap", () => {
    const bad: string[] = [];
    for (const r of all) {
      if (!/(^|[\s,>])(th|td)\b/.test(r.selector)) continue;
      const pad = decl(r.body, "padding");
      const sides = [decl(r.body, "padding-inline"), decl(r.body, "padding-left"), decl(r.body, "padding-right")];
      if (pad) {
        const p = parts(pad);
        sides.push(p[1] ?? p[0], p[3] ?? p[1] ?? p[0]);
      }
      for (const s of sides) {
        if (s === null) continue;
        for (const v of parts(s)) {
          const n = px(v);
          if (n !== null && n < MIN_PX / 2) bad.push(`${r.file} ${r.selector}: ${s}`);
        }
      }
    }
    expect(bad).toEqual([]);
  });

  it("scrollbars take their own width (no overlay over the last column)", () => {
    expect(SOURCES["App.svelte"]).toMatch(/:global\(\*::-webkit-scrollbar\)\s*\{\s*width: 12px;/);
  });

  it("the app sets the gap and pads every table cell with it", () => {
    const app = SOURCES["App.svelte"];
    expect(app).toMatch(/--col-gap:\s*6px;/);
    expect(app).toMatch(/:global\(:where\(th, td\)\)\s*\{\s*padding-inline: calc\(var\(--col-gap, 6px\) \/ 2\);/);
  });
});
