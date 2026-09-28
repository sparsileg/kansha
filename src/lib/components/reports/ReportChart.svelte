<script lang="ts">
  // A bar and line graph (RPT-010). Rust places every value on a 0–10000
  // scale; this only turns positions into pixels. Series differ by shape
  // and pattern as well as color (solid bars, hatched bars, a line with
  // square marks), so they read without color vision.
  import { displayDate, monthShort } from "../../format/date";
  import { formatMoney } from "../../format/money";
  import type { Chart } from "../../types/bindings";

  let { chart, height = 260 }: { chart: Chart; height?: number } = $props();

  const W = 760;
  const LEFT = 56;
  const RIGHT = 12;
  const TOP = 10;
  const BOTTOM = 26;
  const plotW = W - LEFT - RIGHT;
  const plotH = $derived(height - TOP - BOTTOM);
  const y = (pos: number) => TOP + plotH - (pos / 10000) * plotH;

  // A graph by category (asset class) names its bars; one over dates
  // shows months.
  const byLabel = $derived(chart.labels.length > 0);
  const n = $derived(Math.max(byLabel ? chart.labels.length : chart.dates.length, 1));
  const xName = (i: number) => (byLabel ? chart.labels[i] : displayDate(chart.dates[i]));
  const xTick = (i: number) => (byLabel ? chart.labels[i] : monthShort(chart.dates[i]));
  const slot = $derived(plotW / n);
  const bars = $derived(chart.series.filter((s) => s.style === "bar"));
  const lines = $derived(chart.series.filter((s) => s.style === "line"));
  const barW = $derived((slot * 0.7) / Math.max(bars.length, 1));
  const cx = (i: number) => LEFT + slot * i + slot / 2;
  // Label every date when they fit, else every k-th.
  const every = $derived(Math.max(1, Math.ceil(n / 12)));

  const FILLS = ["url(#k-solid-1)", "url(#k-hatch-2)", "url(#k-dots-3)"];
  const barFill = (i: number) => FILLS[i % FILLS.length];
</script>

<figure class="chart">
  <svg viewBox="0 0 {W} {height}" role="img" aria-label="Graph: {chart.series.map((s) => s.name).join(', ')}">
    <defs>
      <pattern id="k-solid-1" width="6" height="6" patternUnits="userSpaceOnUse">
        <rect width="6" height="6" fill="var(--chart-1)" />
      </pattern>
      <pattern id="k-hatch-2" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
        <rect width="6" height="6" fill="var(--chart-2-light)" />
        <rect width="3" height="6" fill="var(--chart-2)" />
      </pattern>
      <pattern id="k-dots-3" width="6" height="6" patternUnits="userSpaceOnUse">
        <rect width="6" height="6" fill="var(--chart-2-light)" />
        <circle cx="3" cy="3" r="1.6" fill="var(--chart-2)" />
      </pattern>
    </defs>

    {#each chart.ticks as t (t.pos)}
      <line class="grid" x1={LEFT} x2={W - RIGHT} y1={y(t.pos)} y2={y(t.pos)} />
      <text class="tick" x={LEFT - 6} y={y(t.pos) + 4} text-anchor="end">{t.label}</text>
    {/each}
    <line class="axis" x1={LEFT} x2={W - RIGHT} y1={y(chart.zero)} y2={y(chart.zero)} />

    {#each bars as s, b (s.name)}
      {#each s.pos as p, i (i)}
        {@const top = Math.min(y(p), y(chart.zero))}
        <rect
          class="bar"
          x={cx(i) - (barW * bars.length) / 2 + b * barW}
          y={top}
          width={Math.max(barW - 1, 1)}
          height={Math.max(Math.abs(y(p) - y(chart.zero)), 0.5)}
          fill={barFill(b)}
        ><title>{s.name}, {xName(i)}: {formatMoney(s.values[i])}</title></rect>
      {/each}
    {/each}

    {#each lines as s (s.name)}
      <polyline class="line" points={s.pos.map((p, i) => `${cx(i)},${y(p)}`).join(" ")} />
      {#each s.pos as p, i (i)}
        <rect class="mark" x={cx(i) - 4} y={y(p) - 4} width="8" height="8"
          ><title>{s.name}, {xName(i)}: {formatMoney(s.values[i])}</title></rect
        >
      {/each}
    {/each}

    {#each { length: n } as _, i (i)}
      {#if i % every === 0 && (byLabel ? i < chart.labels.length : i < chart.dates.length)}
        <text class="tick" x={cx(i)} y={height - 8} text-anchor="middle">{xTick(i)}</text>
      {/if}
    {/each}
  </svg>
  <figcaption class="legend">
    {#each bars as s, b (s.name)}
      <span><svg width="14" height="14" aria-hidden="true"><rect width="14" height="14" fill={barFill(b)} stroke="currentColor" stroke-width="0.5" /></svg>{s.name}</span>
    {/each}
    {#each lines as s (s.name)}
      <span><svg width="22" height="14" aria-hidden="true"><line class="line" x1="0" x2="22" y1="7" y2="7" /><rect class="mark" x="7" y="3" width="8" height="8" /></svg>{s.name}</span>
    {/each}
  </figcaption>
</figure>

<style>
  .chart {
    margin: 0.5rem auto 1rem;
    max-width: 52rem;
  }
  svg {
    width: 100%;
    height: auto;
    display: block;
    overflow: visible;
  }
  .grid {
    stroke: currentColor;
    stroke-opacity: 0.15;
  }
  .axis {
    stroke: currentColor;
    stroke-opacity: 0.6;
  }
  .tick {
    fill: currentColor;
    font-size: 11px;
    opacity: 0.8;
  }
  .bar {
    stroke: currentColor;
    stroke-opacity: 0.35;
    stroke-width: 0.5;
  }
  .line {
    fill: none;
    stroke: var(--chart-line);
    stroke-width: 2;
  }
  .mark {
    fill: var(--chart-line);
    stroke: var(--bg, #fff);
    stroke-width: 1;
  }
  .legend {
    display: flex;
    gap: 1.2rem;
    justify-content: center;
    font-size: 0.9em;
  }
  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
  }
</style>
