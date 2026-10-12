// Every label is in sentence case: only the first word is capitalized,
// apart from acronyms and names. Checks buttons, dialog titles, headings,
// field labels, options, column headers in markup, context menu and tab
// labels, tooltips, aria labels, and placeholders. Title Case stays only
// where Quicken has it: the menu bar, report names, and the report and
// portfolio column headings (files in TITLE_CASE_FILES). Reads every
// component's source, since the labels are in the markup.

import { describe, expect, it } from "vitest";

// Capitalized words allowed after the first: names, not ordinary words.
const PROPER = new Set(["Kansha", "Quicken", "Roth", "Downloads"]);

// Whole labels that name something whose own name is in Title Case.
const NAMES = new Set([
  "Net Worth report", // the report's name (Reports menu)
  "Memorized Payees", // tab named as in the Tools menu
  "Balance Adjustment…", // the built-in category (RCN-040)
  "Monthly Avg", // spending card column, as in Quicken (CARD-060)
  "Count money moved to or from accounts not chosen on the Accounts tab", // a tab's name
  "Enter creates the tag; Esc cancels", // key names
]);

// Labels in these files are Title Case on purpose, as in Quicken.
const TITLE_CASE_FILES = new Set([
  "lib/shell/menus.ts", // the menu bar
  "lib/reports/meta.ts", // report names
  "lib/invest/views.ts", // portfolio column headings
  "lib/state/theme.svelte.ts", // font names
]);

// Every component's and module's source, by path from src/.
const SOURCES = Object.fromEntries(
  Object.entries(import.meta.glob<string>("/src/**/*.{svelte,ts}", { query: "?raw", import: "default", eager: true }))
    .map(([path, src]): [string, string] => [path.slice("/src/".length), src])
    .filter(([path]) => !path.includes(".test.") && !path.startsWith("lib/types/")),
);

// Elements whose text is a label.
const LABEL_ELEMENTS = ["button", "h1", "h2", "h3", "h4", "h5", "h6", "legend", "label", "option", "summary", "th", "caption"];

// Attributes whose value is a label or tooltip.
const LABEL_ATTRIBUTES = ["title", "aria-label", "placeholder", "label"];

/** Index just past the `>` that ends the tag opened at `from`, skipping
 * `>` inside quotes and `{…}` expressions (`() => x`). */
function tagEnd(src: string, from: number): number {
  let depth = 0;
  let quote: string | null = null;
  for (let i = from; i < src.length; i++) {
    const c = src[i];
    if (quote !== null) {
      if (c === quote) quote = null;
    } else if (depth === 0 && (c === '"' || c === "'")) {
      quote = c;
    } else if (c === "{") {
      depth++;
    } else if (c === "}") {
      depth--;
    } else if (c === ">" && depth === 0) {
      return i + 1;
    }
  }
  return src.length;
}

/** Markup with each tag replaced by a space. */
function stripTags(markup: string): string {
  let out = "";
  let i = 0;
  while (i < markup.length) {
    const lt = markup.indexOf("<", i);
    if (lt < 0) return out + markup.slice(i);
    out += markup.slice(i, lt) + " ";
    i = tagEnd(markup, lt);
  }
  return out;
}

/** Markup text with each top-level `{…}` expression replaced by a
 * space, and the expressions themselves. */
function splitExpressions(inner: string): { text: string; expressions: string[] } {
  let text = "";
  const expressions: string[] = [];
  let depth = 0;
  let current = "";
  for (const c of inner) {
    if (c === "{") depth++;
    if (depth > 0) current += c;
    else text += c;
    if (c === "}" && --depth === 0) {
      expressions.push(current);
      current = "";
      text += " ";
    }
  }
  return { text, expressions };
}

/** The string literals in an expression, `${…}` parts dropped. Literals
 * compared against (`=== "x"`) or used as keys are not labels. */
function literals(expression: string): string[] {
  return [...expression.replace(/\$\{[^{}]*\}/g, " ").matchAll(/(===?|!==?)?\s*(?:"([^"]*)"|`([^`]*)`)/g)]
    .filter((s) => s[1] === undefined)
    .map((s) => s[2] ?? s[3]);
}

function decode(text: string): string {
  return text.replace(/&gt;/g, ">").replace(/&lt;/g, "<").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();
}

/** The text pieces of each label element's content: the markup text, and
 * each string literal inside a `{…}` expression in it. A label's own
 * text ends where a nested field begins. */
export function elementLabels(src: string, tag = "button"): string[] {
  const out: string[] = [];
  for (const m of src.matchAll(new RegExp(`<${tag}\\b`, "g"))) {
    const start = tagEnd(src, m.index);
    const end = src.indexOf(`</${tag}>`, start);
    if (end < 0) continue;
    let inner = src.slice(start, end);
    const field = inner.search(/<(select|input|textarea|button|[A-Z])/);
    if (field >= 0) inner = inner.slice(0, field);
    const { text, expressions } = splitExpressions(inner);
    for (const piece of [stripTags(text), ...expressions.flatMap(literals)]) {
      const t = decode(piece);
      if (/[A-Za-z]/.test(t)) out.push(t);
    }
  }
  return out;
}

/** Each label attribute's value: a quoted string, or the literals of a
 * `{…}` expression. */
export function attributeLabels(src: string): string[] {
  const out: string[] = [];
  for (const name of LABEL_ATTRIBUTES) {
    for (const m of src.matchAll(new RegExp(`\\s${name}=("([^"]*)"|\\{)`, "g"))) {
      if (m[2] !== undefined) {
        out.push(splitExpressions(m[2]).text);
      } else {
        const open = m.index + m[0].length - 1;
        out.push(...literals(splitExpressions(src.slice(open, tagEnd(src, open))).expressions[0] ?? ""));
      }
    }
  }
  return out.map(decode).filter((t) => /[A-Za-z]/.test(t));
}

/** `label: "…"` entries in scripts: context menus, tabs, choices. */
export function scriptLabels(src: string): string[] {
  return [...src.matchAll(/\blabel:\s*"([^"]+)"/g)].map((m) => m[1]);
}

/** Words after the first that are capitalized but are neither acronyms
 * nor proper names. A word that starts a new sentence may be capitalized. */
export function titleCaseWords(label: string): string[] {
  if (NAMES.has(label)) return [];
  const words = label.split(/\s+/).filter((w) => /[A-Za-z.!?;:]/.test(w));
  return words.slice(1).filter((w, i) => {
    const word = w.replace(/[^A-Za-z]/g, "");
    if (!/^[A-Z]/.test(word)) return false;
    if (/[.!?:]$/.test(words[i])) return false;
    if (/^[A-Z]{2,}s?$/.test(word) || /^[A-Z]\d/.test(w)) return false;
    return !PROPER.has(word);
  });
}

function allLabels(file: string, src: string): string[] {
  const markup = file.endsWith(".svelte") ? src : "";
  return [
    ...LABEL_ELEMENTS.flatMap((tag) => elementLabels(markup, tag)),
    ...attributeLabels(markup),
    ...(TITLE_CASE_FILES.has(file) ? [] : scriptLabels(src)),
  ];
}

describe("labels", () => {
  it("finds labels, including ones inside expressions and after arrow handlers", () => {
    const src =
      `<button onclick={() => go()}>Clear All</button>
      <button>{busy ? "Saving…" : "Save As"}</button>` + "\n<button>{all ? `All ${n} Rows` : \"Rows\"}</button>";
    expect(elementLabels(src)).toEqual(["Clear All", "Saving…", "Save As", "All Rows", "Rows"]);
  });

  it("finds titles, field labels, and menu entries, not the fields inside", () => {
    const src = `<Modal title="Find And Replace" onclose={() => x}>
      <Modal title={a === "b" ? "Edit Thing" : "New thing"}>
      <label>Transaction Type <select><option>Market Value</option></select></label>
      <script>const items = [{ label: "Edit Account Details…" }];</script>`;
    expect(attributeLabels(src)).toEqual(["Find And Replace", "Edit Thing", "New thing"]);
    expect(elementLabels(src, "label")).toEqual(["Transaction Type"]);
    expect(elementLabels(src, "option")).toEqual(["Market Value"]);
    expect(scriptLabels(src)).toEqual(["Edit Account Details…"]);
  });

  it("flags title case but not acronyms, names, or a new sentence", () => {
    expect(titleCaseWords("Reset View")).toEqual(["View"]);
    expect(titleCaseWords("Export CSV")).toEqual([]);
    expect(titleCaseWords("Choose QIF file…")).toEqual([]);
    expect(titleCaseWords("Use Downloads")).toEqual([]);
    expect(titleCaseWords("Enter/Done")).toEqual([]);
    expect(titleCaseWords("and more. Show details")).toEqual([]);
    expect(titleCaseWords("Interest: Enter it")).toEqual([]);
  });

  it("are in sentence case in every component", () => {
    const bad = Object.entries(SOURCES).flatMap(([file, src]) =>
      allLabels(file, src)
        .filter((l) => titleCaseWords(l).length > 0)
        .map((l) => `${file}: ${l}`),
    );
    expect(bad).toEqual([]);
  });
});
