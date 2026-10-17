<script lang="ts">
  // A report's table: the label column, then the report's columns.
  // Groups collapse and expand; a figure with a source opens it (RPT-030).
  // With `sort`, the Date, Account, and Num headings sort the report.
  // The heading row stays at the top while the page scrolls, and repeats
  // on every printed page.
  //
  // A compact report (RPT-145) has no label column: a group's label spans
  // the columns up to its figures, and a transaction's first column sits
  // under it. Top-level rows are shaded; every row is one line, text cut
  // with "…" to fit the page (`fitColumns`), on screen and on paper.
  //
  // A long report draws only the lines in view, plus a margin, between
  // two spacer rows standing for the rest; Save PDF draws every line.
  import { tick, untrack } from "svelte";
  import { columnHeading, formatCell } from "../../format/report";
  import {
    CUT_ORDER,
    NOTE_PT,
    PAGE_HEIGHT_PT,
    PAGE_WIDTH_PT,
    PRINT_FONT_PT,
    TITLE_PT,
    WIDTH_SLACK,
    fitColumns,
    paginate,
    type Fit,
  } from "../../reports/fit";
  import { COLUMN_SORTS, REPORTS } from "../../reports/meta";
  import { flatten, type Line } from "../../reports/rows";
  import type { ReportInstance } from "../../state/reports.svelte";
  import type { Column, DetailSort, PageOrientation, Report } from "../../types/bindings";

  let {
    report,
    inst,
    ondrill,
    sort = null,
    onsort,
    orientation = "portrait",
  }: {
    report: Report;
    /** Which groups are collapsed. */
    inst: Pick<ReportInstance, "isCollapsed" | "toggle">;
    ondrill: (line: Line, column: Column | null) => void;
    sort?: { by: DetailSort; desc: boolean } | null;
    onsort?: (by: DetailSort) => void;
    /** The paper the compact layout fits on Save PDF. */
    orientation?: PageOrientation;
  } = $props();

  const sortOf = (c: Column): DetailSort | null => (sort && onsort ? (COLUMN_SORTS[c.id] ?? null) : null);

  const WINDOW_FROM = 500;
  const OVERSCAN = 40;
  const lines = $derived(flatten(report.rows, (p) => inst.isCollapsed(p), report.totals_on_heading));
  const numeric = (c: Column) => c.kind === "money" || c.kind === "quantity" || c.kind === "percent";
  /** Negative amounts show in the theme's negative color, as Quicken's
   * red (RPT-050); the minus sign stays. */
  const negative = (c: Column, text: string) => c.kind === "money" && text.startsWith("-");

  // Compact layout.
  /** A group's label spans the columns before the first figure. */
  const span = $derived.by(() => {
    const i = report.columns.findIndex(numeric);
    return i < 0 ? report.columns.length : Math.max(1, i);
  });
  const labelled = (l: Line) => l.path !== null || l.label !== "";
  /** A row's first-column indent, in em. */
  const indent = (depth: number) => em(0.4 + depth * 1.3);
  /** The first heading lines up with the transactions under it. */
  const headIndent = $derived.by(() => {
    const d = lines.find((l) => !labelled(l));
    return d ? indent(d.depth) : null;
  });
  const cuttable = new Set(CUT_ORDER.flat());
  const fontPx = (el: HTMLElement) => parseFloat(getComputedStyle(el).fontSize) || 16;
  let table = $state<HTMLTableElement>();
  let roomEl = $state<HTMLDivElement>();
  /** The page's width for the table, in px. */
  let room = $state(0);
  const ids = $derived(report.columns.map((c) => c.id));
  /** Natural column widths in em, measured with nothing cut, and the
   * columns they were measured for. */
  let measured = $state<{ ids: string[]; widths: number[] } | null>(null);
  /** Null while measuring, and when the columns changed since (Customize):
   * old widths would not match the new columns. */
  const natural = $derived(
    measured && measured.ids.length === ids.length && measured.ids.every((id, i) => id === ids[i])
      ? measured.widths
      : null,
  );
  const screen = $derived<Fit | null>(natural && roomEl ? fitColumns(ids, natural, room / fontPx(roomEl)) : null);
  const paper = $derived<Fit | null>(natural ? fitColumns(ids, natural, PAGE_WIDTH_PT[orientation] / PRINT_FONT_PT) : null);
  const em = (n: number) => `${n.toFixed(3)}em`;
  const total = (xs: number[]) => xs.reduce((a, b) => a + b, 0);
  /** Each printed page's first line, while saving a PDF. */
  let pages = $state<number[] | null>(null);

  // Natural widths come from every line with nothing collapsed, so
  // collapsing or expanding never needs them again (a collapsed group
  // never needs more room). A long report measures only the lines with
  // the longest text in each column.
  const allLines = $derived(flatten(report.rows, () => false, report.totals_on_heading));
  const measureLines = $derived(allLines.length > WINDOW_FROM ? widest(allLines) : allLines);

  /** The few lines with the longest text in each column, and the
   * deepest (their indent adds width). */
  function widest(all: Line[]): Line[] {
    const keep = new Set<Line>();
    const top = (score: (l: Line) => number) => {
      for (const l of [...all].sort((a, b) => score(b) - score(a)).slice(0, 5)) keep.add(l);
    };
    report.columns.forEach((c, i) => {
      top((l) => {
        if (labelled(l) && i < span) return -1;
        const text = formatCell(c.kind, l.cells[i] ?? "", report.cents).length;
        return i === 0 ? text + l.depth * 2.6 : text;
      });
    });
    top((l) => l.depth);
    return all.filter((l) => keep.has(l));
  }

  // Measure again when the report or its columns change.
  $effect(() => {
    void report;
    void ids;
    if (!report.compact) return;
    untrack(() => {
      measured = null;
      void tick().then(measure);
    });
  });

  // Windowing: a report of more lines than this draws only those in view.
  /** Set while Save PDF needs every line drawn. */
  let printing = $state(false);
  const windowed = $derived(!printing && lines.length > WINDOW_FROM);
  /** A line's drawn height in px by its look (kind, heading, total);
   * `rowH` until one of that look has been drawn. */
  let heights = $state<Record<string, number>>({});
  let rowH = $state(22);
  const look = (l: Line) => `${l.kind}${l.path !== null && !l.collapsed ? "h" : ""}${l.closing ? "c" : ""}`;
  /** Each line's top in px within the body, and the body's height last. */
  const tops = $derived.by(() => {
    if (!windowed) return [];
    const out = new Array<number>(lines.length + 1);
    out[0] = 0;
    for (let i = 0; i < lines.length; i++) out[i + 1] = out[i] + (heights[look(lines[i])] ?? rowH);
    return out;
  });
  /** The visible part of the body, in px from its top. */
  let view = $state({ top: 0, height: 800 });
  /** The first line whose bottom is below `y`. */
  const lineAt = (y: number) => {
    let lo = 0;
    let hi = lines.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (tops[mid + 1] <= y) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  };
  const range = $derived.by(() => {
    if (!windowed) return { from: 0, to: lines.length };
    const from = Math.max(0, lineAt(view.top) - OVERSCAN);
    const to = Math.min(lines.length, lineAt(view.top + view.height) + 1 + OVERSCAN);
    return { from, to };
  });
  const shown = $derived(natural === null && report.compact ? measureLines : lines.slice(range.from, range.to));
  let body = $state<HTMLTableSectionElement>();

  /** The nearest ancestor that scrolls, or none. */
  function scrollerOf(el: HTMLElement): HTMLElement | null {
    for (let p = el.parentElement; p; p = p.parentElement) {
      const o = getComputedStyle(p).overflowY;
      if (o === "auto" || o === "scroll") return p;
    }
    return null;
  }

  // Follow the scrolling while windowed.
  $effect(() => {
    const el = body;
    if (!el || !windowed) return;
    const scroller = scrollerOf(el);
    const update = () => {
      const box = scroller?.getBoundingClientRect() ?? { top: 0 };
      const height = scroller?.clientHeight || window.innerHeight || 800;
      view = { top: Math.max(0, box.top - el.getBoundingClientRect().top), height };
    };
    update();
    const target: HTMLElement | Window = scroller ?? window;
    target.addEventListener("scroll", update, { passive: true });
    window.addEventListener("resize", update);
    return () => {
      target.removeEventListener("scroll", update);
      window.removeEventListener("resize", update);
    };
  });

  // Learn each look's height from the drawn lines.
  $effect(() => {
    void shown;
    if (!windowed) return;
    void tick().then(() => {
      const trs = body ? [...body.querySelectorAll<HTMLElement>("tr[data-i]")] : [];
      const next = { ...untrack(() => heights) };
      let changed = false;
      for (const tr of trs) {
        const h = tr.getBoundingClientRect().height;
        const l = lines[Number(tr.dataset.i)];
        if (h > 0 && l && next[look(l)] !== h) {
          next[look(l)] = h;
          changed = true;
        }
      }
      if (changed) {
        heights = next;
        rowH = Object.values(next)[0] ?? rowH;
      }
    });
  });

  $effect(() => {
    const el = roomEl;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => (room = el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  });

  /** Natural widths, measured at the paper's text size (set inline while
   * `.measuring`). */
  function measure() {
    if (!table) return;
    const px = fontPx(table);
    measured = {
      ids,
      widths: [...table.querySelectorAll<HTMLElement>("thead th")].map(
        (th) => (th.getBoundingClientRect().width / px) * WIDTH_SLACK,
      ),
    };
  }

  /** Before Save PDF: split a compact report into pages (`paginate`),
   * from its rows' heights set as on paper (`.paper`). */
  export async function preparePrint(): Promise<void> {
    printing = true;
    await tick();
    if (!report.compact || !table || !paper || lines.length === 0) return;
    const t = table;
    t.classList.add("paper");
    t.style.fontSize = `${PRINT_FONT_PT * paper.scale}pt`;
    // CSS px to pt.
    const pt = (el: Element) => el.getBoundingClientRect().height * 0.75;
    const trs = [...t.querySelectorAll("tbody tr")];
    const rows = lines.map((l, i) => ({ h: pt(trs[i]), heading: l.path !== null && !l.collapsed }));
    const head = t.tHead ? pt(t.tHead) : 0;
    t.classList.remove("paper");
    t.style.fontSize = "";
    pages = paginate(rows, head, PAGE_HEIGHT_PT[orientation], TITLE_PT + (report.note ? NOTE_PT : 0) + (REPORTS[report.kind].compare ? NOTE_PT : 0));
    await tick();
  }

  /** After Save PDF. */
  export function endPrint() {
    pages = null;
    printing = false;
  }
</script>

{#snippet headRow()}
  <tr>
    {#each report.columns as c (c.id)}
      {@const by = sortOf(c)}
      {@const on = by !== null && sort?.by === by}
      <th
        class:num={numeric(c)}
        aria-sort={on ? (sort?.desc ? "descending" : "ascending") : undefined}
        style:padding-left={c === report.columns[0] && !numeric(c) ? headIndent : null}
      >
        {#if by !== null}
          <button
            type="button"
            class="sort"
            title={on && !sort?.desc ? `Sort by ${c.label}, descending` : `Sort by ${c.label}, ascending`}
            onclick={() => onsort?.(by)}
          >{c.label}<span class="dir no-print" aria-hidden="true">{on ? (sort?.desc ? " ▼" : " ▲") : " ⇅"}</span></button>
        {:else}
          {#each columnHeading(c) as part, i (i)}{#if i > 0}<br />{/if}{part}{/each}
        {/if}
      </th>
    {/each}
  </tr>
{/snippet}

{#snippet gap(px: number, cols: number)}
  {#if px > 0}<tr class="gap" aria-hidden="true"><td colspan={cols} style:height="{px}px"></td></tr>{/if}
{/snippet}

{#snippet bodyRow(l: Line, i: number | null = null)}
  <tr class={l.kind} class:closing={l.closing} class:heading={l.path !== null && !l.collapsed} data-i={i}>
    {#if labelled(l)}
      <td class="label" colspan={span} title={l.label} style:padding-left={indent(l.depth)}>
        {#if l.path !== null}
          <button
            type="button"
            class="tog"
            aria-expanded={!l.collapsed}
            aria-label={l.collapsed ? `Expand ${l.label}` : `Collapse ${l.label}`}
            onclick={() => inst.toggle(l.path!)}
          >{l.collapsed ? "▸" : "▾"}</button>
        {/if}<span class="text">{l.label}</span>
      </td>
    {/if}
    {#each report.columns as c, i (c.id)}
      {#if !labelled(l) || i >= span}
        {@const text = formatCell(c.kind, l.cells[i] ?? "", report.cents)}
        <td
          class:num={numeric(c)}
          class:neg={negative(c, text)}
          title={cuttable.has(c.id) && text ? text : undefined}
          style:padding-left={i === 0 && !labelled(l) ? indent(l.depth) : null}
        >
          {#if text && l.drill && c.kind === "money"}
            <button type="button" class="fig" title="Show where this comes from" onclick={() => ondrill(l, c)}>{text}</button>
          {:else if text && l.drill && l.kind === "detail" && i === 0}
            <button type="button" class="fig" title="Show this transaction" onclick={() => ondrill(l, null)}>{text}</button>
          {:else}
            {text}
          {/if}
        </td>
      {/if}
    {/each}
  </tr>
{/snippet}

{#snippet cols()}
  {#if screen && paper}
    <colgroup>
      {#each report.columns as c, i (c.id)}<col style:--w={em(screen.widths[i])} style:--wp={em(paper.widths[i])} />{/each}
    </colgroup>
  {/if}
{/snippet}

{#if report.compact}
  <div bind:this={roomEl}>
    <table
      class="report compact"
      class:measuring={natural === null}
      class:no-print={pages !== null}
      style:font-size={natural === null ? `${PRINT_FONT_PT}pt` : null}
      bind:this={table}
      style:--tw={screen ? em(total(screen.widths)) : null}
      style:--twp={paper ? em(total(paper.widths)) : null}
      style:--ps={paper?.scale ?? 1}
    >
      {@render cols()}
      <thead>{@render headRow()}</thead>
      <tbody bind:this={body}>
        {#if windowed && natural !== null}{@render gap(tops[range.from], report.columns.length)}{/if}
        {#each shown as l, k (l.key)}
          {@render bodyRow(l, natural === null ? null : range.from + k)}
        {:else}
          <tr><td class="none" colspan={report.columns.length}>Nothing to report for these dates and settings.</td></tr>
        {/each}
        {#if windowed && natural !== null}{@render gap(tops[lines.length] - tops[range.to], report.columns.length)}{/if}
      </tbody>
    </table>
  </div>
  {#if pages && paper}
    <!-- Paper: one table per page, each with the column headings. -->
    <div class="pages">
      {#each pages as from, p (from)}
        <div class="paper-page">
          <table class="report compact" style:--twp={em(total(paper.widths))} style:--ps={paper.scale}>
            {@render cols()}
            <thead>{@render headRow()}</thead>
            <tbody>
              {#each lines.slice(from, pages[p + 1] ?? lines.length) as l (l.key)}{@render bodyRow(l)}{/each}
            </tbody>
          </table>
        </div>
      {/each}
    </div>
  {/if}
{:else}
<table class="report">
  <thead>
    <tr>
      <th class="label"></th>
      {#each report.columns as c (c.id)}
        {@const by = sortOf(c)}
        {@const on = by !== null && sort?.by === by}
        <th class:num={numeric(c)} aria-sort={on ? (sort?.desc ? "descending" : "ascending") : undefined}>
          {#if by !== null}
            <button
              type="button"
              class="sort"
              title={on && !sort?.desc ? `Sort by ${c.label}, descending` : `Sort by ${c.label}, ascending`}
              onclick={() => onsort?.(by)}
            >{c.label}<span class="dir no-print" aria-hidden="true">{on ? (sort?.desc ? " ▼" : " ▲") : " ⇅"}</span></button>
          {:else}
            {#each columnHeading(c) as part, i (i)}{#if i > 0}<br />{/if}{part}{/each}
          {/if}
        </th>
      {/each}
    </tr>
  </thead>
  <tbody bind:this={body}>
    {#if windowed}{@render gap(tops[range.from], report.columns.length + 1)}{/if}
    {#each shown as l, k (l.key)}
      <tr class={l.kind} class:closing={l.closing} class:heading={l.path !== null && !l.collapsed} data-i={range.from + k}>
        <td class="label" style="padding-left: {0.4 + l.depth * 1.2}rem">
          {#if l.path !== null}
            <button
              type="button"
              class="tog no-print"
              aria-expanded={!l.collapsed}
              aria-label={l.collapsed ? `Expand ${l.label}` : `Collapse ${l.label}`}
              onclick={() => inst.toggle(l.path!)}
            >{l.collapsed ? "▸" : "▾"}</button>
          {/if}{l.label}
        </td>
        {#each report.columns as c, i (c.id)}
          {@const text = formatCell(c.kind, l.cells[i] ?? "", report.cents)}
          <td class:num={numeric(c)} class:neg={negative(c, text)}>
            {#if text && l.drill && c.kind === "money"}
              <button type="button" class="fig" title="Show where this comes from" onclick={() => ondrill(l, c)}>{text}</button>
            {:else if text && l.drill && l.kind === "detail" && i === 0}
              <button type="button" class="fig" title="Show this transaction" onclick={() => ondrill(l, null)}>{text}</button>
            {:else}
              {text}
            {/if}
          </td>
        {/each}
      </tr>
    {:else}
      <tr><td class="none" colspan={report.columns.length + 1}>Nothing to report for these dates and settings.</td></tr>
    {/each}
    {#if windowed}{@render gap(tops[lines.length] - tops[range.to], report.columns.length + 1)}{/if}
  </tbody>
</table>
{/if}

<style>
  .report {
    border-collapse: collapse;
    margin: 0 auto;
  }
  th,
  td {
    text-align: left;
    padding: 0.1rem 0.6rem;
    white-space: nowrap;
    vertical-align: bottom;
  }
  thead th {
    border-bottom: 1px solid var(--line);
    font-weight: 600;
    position: sticky;
    top: 0;
    z-index: 2;
    background: var(--bg);
  }
  @media print {
    thead {
      display: table-header-group;
    }
    thead th {
      position: static;
    }
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .label {
    min-width: 10rem;
  }
  /* Sections in capitals and bold; group headings bold; totals ruled. */
  .section td.label {
    font-weight: 700;
    padding-top: 0.5rem;
  }
  .group.heading td.label,
  .group.closing td,
  .section.closing td {
    font-weight: 600;
  }
  .closing td.num,
  .total td.num {
    border-top: 1px solid var(--line);
  }
  .total td {
    font-weight: 700;
    padding-top: 0.5rem;
  }
  .tog {
    width: 1.3rem;
    padding: 0;
    margin-right: 0.15rem;
    background: none;
    border: none;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  .sort {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  .sort:hover {
    text-decoration: underline;
  }
  .dir {
    font-size: var(--fs-small);
  }
  .fig {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: inherit;
    cursor: pointer;
    text-align: inherit;
  }
  .fig:hover {
    text-decoration: underline;
  }
  .neg {
    color: var(--bad);
  }
  .none {
    opacity: 0.7;
    padding: 1rem;
  }
  /* Stands for the lines not drawn. */
  .gap td {
    padding-top: 0;
    padding-bottom: 0;
    border: 0;
  }

  /* Compact (RPT-145): fixed column widths from the fit, one line per
     row, text cut with "…"; while measuring, natural widths and a
     group's label adds no width. Sizes in em so paper scales them. */
  .compact {
    /* Set inline from the fit: widths on screen and paper, paper's
       scale. */
    --tw: auto;
    --twp: auto;
    --ps: 1;
    table-layout: fixed;
    width: var(--tw);
    margin: 0;
  }
  .compact.measuring {
    table-layout: auto;
    width: auto;
  }
  .compact.measuring td.label .text {
    display: inline-block;
    width: 0;
  }
  .compact col {
    --w: auto;
    --wp: auto;
    width: var(--w);
  }
  .compact th,
  .compact td {
    padding: 0.1em 0.5em;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .compact td.label {
    min-width: 0;
  }
  .compact .tog {
    width: 1.15em;
    margin-right: 0.15em;
  }
  /* Top-level rows (a tax form) shaded. */
  .compact tr.section td {
    background: var(--report-shade);
    padding-top: 0.15em;
    print-color-adjust: exact;
    -webkit-print-color-adjust: exact;
  }
  /* Form and group headings bold across the row, totals too. */
  .compact tr.section td {
    font-weight: 700;
  }
  .compact tr.group.heading td {
    font-weight: 600;
  }
  .compact tr.total td {
    padding-top: 0.3em;
  }
  /* Paper's widths on screen, to measure the rows (with paper's text
     size, set inline). `paper` is added by script while measuring, so
     Svelte cannot see it. */
  .compact:global(.paper) {
    width: var(--twp);
  }
  .compact:global(.paper) col {
    width: var(--wp);
  }
  @media print {
    .compact {
      width: var(--twp);
      font-size: calc(1em * var(--ps));
    }
    .compact col {
      width: var(--wp);
    }
    /* Keep the toggle's room so dates stay under their headings. */
    .compact .tog {
      visibility: hidden;
    }
    .pages {
      display: block;
    }
    .paper-page {
      break-after: page;
    }
    .paper-page:last-child {
      break-after: auto;
    }
  }
  @media screen {
    .pages {
      display: none;
    }
  }
</style>
