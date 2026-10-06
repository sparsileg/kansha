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

  // Measure again whenever the lines change.
  $effect(() => {
    void lines;
    void ids;
    if (!report.compact) return;
    untrack(() => {
      measured = null;
      void tick().then(measure);
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

{#snippet bodyRow(l: Line)}
  <tr class={l.kind} class:closing={l.closing} class:heading={l.path !== null && !l.collapsed}>
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
      <tbody>
        {#each lines as l (l.key)}
          {@render bodyRow(l)}
        {:else}
          <tr><td class="none" colspan={report.columns.length}>Nothing to report for these dates and settings.</td></tr>
        {/each}
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
  <tbody>
    {#each lines as l (l.key)}
      <tr class={l.kind} class:closing={l.closing} class:heading={l.path !== null && !l.collapsed}>
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
