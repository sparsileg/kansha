<script lang="ts">
  // The summary band over both budget views (BUD-040): expenses only,
  // for the range shown. Over budget is said in words, and its bar is
  // striped, never by color alone.
  import { formatMoneyWhole } from "../../format/money";
  import type { Summary } from "../../types/bindings";

  let { summary }: { summary: Summary } = $props();
</script>

<div class="summary" aria-label="Spending summary">
  <div class="spent">
    <span class="cap">SPENDING</span>
    <strong>${formatMoneyWhole(summary.spending)}</strong>
    <span class="range">{summary.range}</span>
  </div>
  <div class="meter">
    <span class="budget-label">Budget: ${formatMoneyWhole(summary.budget)}</span>
    <div class="track">
      {#if summary.bar}
        {@const b = summary.bar}
        <div class="fill" class:over={b.split !== null} style:width="{b.fill / 10}%"></div>
        {#if b.split !== null}<div class="mark" style:left="{b.split / 10}%"></div>{/if}
      {/if}
    </div>
    <span class:over-text={summary.over}>
      {#if summary.over}⚠ ${formatMoneyWhole(summary.amount)} OVER{:else}${formatMoneyWhole(summary.amount)} LEFT{/if}
    </span>
  </div>
</div>

<style>
  .over-text {
    color: var(--bad);
    font-weight: 700;
  }
  .summary {
    display: flex;
    gap: 1.5rem;
    align-items: center;
    padding: 0.4rem 0.6rem;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .spent {
    display: grid;
  }
  .cap {
    font-size: var(--fs-header);
    letter-spacing: 0.05em;
  }
  .range {
    font-size: var(--fs-register);
    opacity: 0.85;
  }
  .meter {
    flex: 1;
    display: grid;
    gap: 0.15rem;
  }
  .budget-label {
    justify-self: end;
  }
  .track {
    position: relative;
    height: 1.1rem;
    border: 1px solid var(--line);
    border-radius: 3px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--good);
  }
  /* Over budget: the whole bar in the "bad" color, crossed with
     stripes so it reads without color; the budget's end marked. */
  .fill.over {
    background: repeating-linear-gradient(45deg, var(--bad) 0 6px, transparent 6px 9px), var(--bad);
    opacity: 0.85;
  }
  .mark {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 3px;
    background: var(--fg);
  }
</style>
