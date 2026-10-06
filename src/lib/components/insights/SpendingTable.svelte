<script lang="ts">
  // A spending card's table (CARD-060): Year and Month count past and
  // scheduled transactions; a figure with scheduled ones in it says how
  // much in its tooltip. With more category rows than the
  // setting allows (`spending_rows`), the rows scroll between a fixed
  // header and Total row, with no scrollbar taking room: the edge with
  // more rows past it fades, and two letter-high arrows pulse there, a
  // third and two thirds across (clicking one scrolls a page). "Rows 1–10 of 24" under the table counts them;
  // clicking it shows every row until clicked again. Printing shows all.
  import { formatMoney, isZeroMoney } from "../../format/money";
  import type { ExpenseCard, ExpenseRow } from "../../types/bindings";

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

  // A figure with scheduled spending in it says how much.
  function sched(r: ExpenseRow): string | undefined {
    return isZeroMoney(r.scheduled) ? undefined : `${formatMoney(r.scheduled)} scheduled`;
  }

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
        <tr><th>Category</th><th class="num">Year</th><th class="num">Month</th><th class="num">Monthly Avg</th></tr>
      </thead>
      <tbody>
        {#each card.rows as r (r.category)}
          <tr>
            <td class="label" title={r.label}>{r.label}</td>
            <td class="num" title={sched(r)}>{formatMoney(r.ytd)}</td>
            <td class="num" title={sched(r)}>{formatMoney(r.mtd)}</td>
            <td class="num">{formatMoney(r.monthly_avg)}</td>
          </tr>
        {/each}
      </tbody>
      <tfoot>
        <tr class="net">
          <td>Total</td>
          <td class="num" title={sched(card.total)}>{formatMoney(card.total.ytd)}</td>
          <td class="num" title={sched(card.total)}>{formatMoney(card.total.mtd)}</td>
          <td class="num">{formatMoney(card.total.monthly_avg)}</td>
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
