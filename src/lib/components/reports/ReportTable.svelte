<script lang="ts">
  // A report's table: the label column, then the report's columns.
  // Groups collapse and expand; a figure with a source opens it (RPT-030).
  // With `sort`, the Date, Account, and Num headings sort the report.
  import { columnHeading, formatCell } from "../../format/report";
  import { COLUMN_SORTS } from "../../reports/meta";
  import { flatten, type Line } from "../../reports/rows";
  import { reportState } from "../../state/reports.svelte";
  import type { Column, DetailSort, Report } from "../../types/bindings";

  let {
    report,
    ondrill,
    sort = null,
    onsort,
  }: {
    report: Report;
    ondrill: (line: Line, column: Column | null) => void;
    sort?: { by: DetailSort; desc: boolean } | null;
    onsort?: (by: DetailSort) => void;
  } = $props();

  const sortOf = (c: Column): DetailSort | null => (sort && onsort ? (COLUMN_SORTS[c.id] ?? null) : null);

  const lines = $derived(flatten(report.rows, (p) => reportState.isCollapsed(p)));
  const numeric = (c: Column) => c.kind === "money" || c.kind === "quantity";
</script>

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
              onclick={() => reportState.toggle(l.path!)}
            >{l.collapsed ? "▸" : "▾"}</button>
          {/if}{l.label}
        </td>
        {#each report.columns as c, i (c.id)}
          {@const text = formatCell(c.kind, l.cells[i] ?? "", report.cents)}
          <td class:num={numeric(c)}>
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
    border-bottom: 1px solid rgba(128, 128, 128, 0.6);
    font-weight: 600;
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
    border-top: 1px solid rgba(128, 128, 128, 0.6);
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
    font-size: 0.8em;
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
  .none {
    opacity: 0.7;
    padding: 1rem;
  }
</style>
