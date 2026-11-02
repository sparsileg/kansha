<script lang="ts">
  // A spending card's table (CARD-060, CARD-070): the columns the card
  // chose, in the order Rust gives them. Year and Month count past and
  // scheduled transactions; a figure with scheduled ones in it says how
  // much in its tooltip, Last Year gives the same period, and each
  // average its months and total. ▲/▼ is Rust's comparison of Monthly
  // Avg with the longer average. With more category rows than the
  // setting allows (`spending_rows`), the rows scroll between a fixed
  // header and Total row, with no scrollbar taking room: the edge with
  // more rows past it fades, and two letter-high arrows pulse there, a
  // third and two thirds across (clicking one scrolls a page). "Rows 1–10 of 24" under the table counts them;
  // clicking it shows every row until clicked again. Printing shows all.
  import { formatMoney, isZeroMoney } from "../../format/money";
  import type { ExpenseCard, ExpenseRow, SpendingColumn } from "../../types/bindings";

  let { card, rows }: { card: ExpenseCard; rows: number } = $props();

  let scroller = $state<HTMLDivElement | null>(null);
  let all = $state(false);
  // Measured, so the height follows the font size.
  let rowH = $state(0);
  let headH = $state(0);
  let footH = $state(0);
  let top = $state(0);
  let room = $state(0);

  const n = $derived(card.rows.length);
  const scrolls = $derived(!all && n > rows);
  const maxHeight = $derived(scrolls && rowH > 0 ? `${headH + rows * rowH + footH}px` : undefined);
  const first = $derived(rowH > 0 ? Math.min(n - rows, Math.round(top / rowH)) + 1 : 1);
  const canUp = $derived(scrolls && top > 1);
  const canDown = $derived(scrolls && top < room - 1);

  function measure() {
    const el = scroller;
    if (!el) return;
    rowH = el.querySelector("tbody tr")?.getBoundingClientRect().height ?? 0;
    headH = el.querySelector("thead")?.getBoundingClientRect().height ?? 0;
    footH = el.querySelector("tfoot")?.getBoundingClientRect().height ?? 0;
    onscroll();
  }

  function onscroll() {
    const el = scroller;
    if (!el) return;
    top = el.scrollTop;
    room = el.scrollHeight - el.clientHeight;
  }

  $effect(() => {
    void [card, rows, all];
    measure();
    // After the new height applies.
    requestAnimationFrame(onscroll);
  });

  const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  /** "2025-10-31" as "Oct 2025". */
  const monthYear = (iso: string) => `${MONTHS[Number(iso.slice(5, 7)) - 1]} ${iso.slice(0, 4)}`;
  const months = (n: number) => (n === 1 ? "1 month" : `${n} months`);

  function head(c: SpendingColumn): string {
    switch (c) {
      case "last_year":
        return "Last Year";
      case "year":
        return "Year";
      case "last_month":
        return "Last Month";
      case "month":
        return "Month";
      case "monthly_avg":
        return "Monthly Avg";
      case "three_year_avg":
        return card.avg_since === null ? "3 Yr Avg" : `Avg since ${monthYear(card.avg_since)}`;
    }
  }

  function figure(c: SpendingColumn, r: ExpenseRow): string {
    switch (c) {
      case "last_year":
        return formatMoney(r.last_year);
      case "year":
        return formatMoney(r.ytd);
      case "last_month":
        return formatMoney(r.last_month);
      case "month":
        return formatMoney(r.mtd);
      case "monthly_avg": {
        const arrow = r.avg_trend === "up" ? "▲ " : r.avg_trend === "down" ? "▼ " : "";
        return arrow + formatMoney(r.monthly_avg);
      }
      case "three_year_avg":
        return formatMoney(r.three_year_avg);
    }
  }

  function tip(c: SpendingColumn, r: ExpenseRow): string | undefined {
    switch (c) {
      case "last_year":
        return `Through ${monthYear(card.same_period_to)}: ${formatMoney(r.same_period)}`;
      case "year":
      case "month":
        // A figure with scheduled spending in it says how much.
        return isZeroMoney(r.scheduled) ? undefined : `${formatMoney(r.scheduled)} scheduled`;
      case "monthly_avg":
        return `${months(card.year_months)}, ${formatMoney(r.ytd)} total`;
      case "three_year_avg":
        return card.avg_months === 0 ? "No earlier months" : `${months(card.avg_months)}, ${formatMoney(r.three_year_total)} total`;
      default:
        return undefined;
    }
  }

  // Past four columns, the figures keep their room.
  const labelWidth = $derived(card.columns.length > 3 ? `${100 - card.columns.length * 13}%` : undefined);

  function page(dir: -1 | 1) {
    scroller?.scrollBy({ top: dir * rows * rowH, behavior: "smooth" });
  }
</script>

<div class="wrap">
  <!-- Focusable while it scrolls, so the arrow and Page keys move it. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="scroller"
    class:scrolls
    bind:this={scroller}
    {onscroll}
    style:max-height={maxHeight}
    tabindex={scrolls ? 0 : undefined}
    role={scrolls ? "region" : undefined}
    aria-label={scrolls ? "Categories" : undefined}
  >
    <table class="expenses">
      <thead>
        <tr>
          <th style:width={labelWidth}>Category</th>
          {#each card.columns as c (c)}<th class="num">{head(c)}</th>{/each}
        </tr>
      </thead>
      <tbody>
        {#each card.rows as r (r.category)}
          <tr>
            <td class="label" title={r.label}>{r.label}</td>
            {#each card.columns as c (c)}<td class="num" title={tip(c, r)}>{figure(c, r)}</td>{/each}
          </tr>
        {/each}
      </tbody>
      <tfoot>
        <tr class="net">
          <td>Total</td>
          {#each card.columns as c (c)}<td class="num" title={tip(c, card.total)}>{figure(c, card.total)}</td>{/each}
        </tr>
      </tfoot>
    </table>
  </div>
  {#if canUp}
    <div class="fade up" style:top={`${headH}px`}>
      {#each ["left", "right"] as side (side)}
        <button type="button" class="arrow {side}" aria-label="Scroll up" tabindex="-1" onclick={() => page(-1)}>▲</button>
      {/each}
    </div>
  {/if}
  {#if canDown}
    <div class="fade down" style:bottom={`${footH}px`}>
      {#each ["left", "right"] as side (side)}
        <button type="button" class="arrow {side}" aria-label="Scroll down" tabindex="-1" onclick={() => page(1)}>▼</button>
      {/each}
    </div>
  {/if}
</div>
{#if n > rows}
  <button type="button" class="count" title={all ? `Show ${rows} rows and scroll` : `Show all ${n} rows`} onclick={() => (all = !all)}>
    {all ? `All ${n} rows` : `Rows ${first}–${first + rows - 1} of ${n}`}
  </button>
{/if}

<style>
  .wrap {
    position: relative;
  }
  /* Scrolls by wheel, touchpad, or keys; no scrollbar takes room. */
  .scroller.scrolls {
    overflow-y: auto;
    scrollbar-width: none;
  }
  .scroller.scrolls::-webkit-scrollbar {
    display: none;
  }
  .scroller:focus-visible {
    outline: 2px solid var(--focus-ring, currentColor);
    outline-offset: 1px;
  }
  table {
    border-collapse: collapse;
    width: 100%;
    table-layout: fixed;
  }
  td,
  th {
    padding: 0.1rem 0.3rem;
  }
  th {
    font-weight: 400;
    text-align: left;
    border-bottom: 1px solid var(--line);
  }
  th:first-child {
    width: 40%;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  th.num {
    text-align: right;
  }
  /* A long category path ends in "…"; the full one is its tooltip. */
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .net td {
    border-top: 1px solid var(--line);
    font-weight: 700;
  }
  /* Header and Total stay put while the rows scroll. */
  .scrolls thead th {
    position: sticky;
    top: 0;
    background: var(--row-bg);
  }
  .scrolls tfoot td {
    position: sticky;
    bottom: 0;
    background: var(--row-bg);
  }
  /* The edge with more rows past it fades out; by opacity, not color. */
  .fade {
    position: absolute;
    left: 0;
    right: 0;
    height: 2em;
    pointer-events: none;
  }
  .fade.up {
    background: linear-gradient(var(--row-bg), transparent);
  }
  .fade.down {
    background: linear-gradient(transparent, var(--row-bg));
  }
  /* Two arrows about a capital letter high, a third and two thirds
     across, pulse from nearly unseen to unseen. */
  .arrow {
    position: absolute;
    top: 0;
    bottom: 0;
    transform: translateX(-50%);
    pointer-events: auto;
    background: none;
    border: none;
    padding: 0 0.8rem;
    font: inherit;
    font-size: var(--fs-arrow);
    line-height: 1;
    color: var(--fg);
    cursor: pointer;
    animation: pulse 2.5s ease-in-out infinite;
  }
  .arrow.left {
    left: 33.33%;
  }
  .arrow.right {
    left: 66.67%;
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 0;
    }
    50% {
      opacity: 0.3;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .arrow {
      animation: none;
      opacity: 0.25;
    }
  }
  .count {
    background: none;
    border: none;
    padding: 0;
    margin-top: 0.3rem;
    font: inherit;
    font-size: var(--fs-register);
    color: inherit;
    opacity: 0.7;
    text-decoration: underline;
    cursor: pointer;
  }
  @media print {
    .scroller.scrolls {
      max-height: none !important;
      overflow: visible;
    }
    .fade,
    .count {
      display: none;
    }
  }
</style>
