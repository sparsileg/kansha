<script lang="ts">
  // A date field with a small calendar popup. Double arrows move one
  // month at a time. Dates are ISO strings; the field shows and reads the
  // user's date format.
  import {
    addMonths,
    dateExample,
    displayDate,
    monthGrid,
    monthLabel,
    monthStart,
    parseDate,
  } from "../../format/date";

  let {
    value,
    today,
    label = "Date",
    onchange,
  }: { value: string; today: string; label?: string; onchange: (iso: string) => void } = $props();

  let open = $state(false);
  let shown = $state("");
  let text = $state("");
  let error = $state(false);
  let root: HTMLDivElement;

  $effect(() => {
    text = displayDate(value);
  });

  function toggle() {
    open = !open;
    if (open) shown = monthStart(value || today);
  }

  function pick(iso: string) {
    open = false;
    error = false;
    onchange(iso);
  }

  function commit() {
    const iso = parseDate(text, today);
    if (iso === null) {
      error = true;
      text = displayDate(value);
      return;
    }
    error = false;
    if (iso !== value) onchange(iso);
  }

  function onwindowclick(e: MouseEvent) {
    if (open && !root.contains(e.target as Node)) open = false;
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && open) {
      e.stopPropagation();
      open = false;
    }
  }
</script>

<svelte:window onclick={onwindowclick} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="date" bind:this={root} {onkeydown}>
  <input
    aria-label={label}
    bind:value={text}
    size="10"
    onchange={commit}
    onkeydown={(e) => e.key === "Enter" && commit()}
  />
  <button type="button" class="cal" aria-label="Show calendar" aria-expanded={open} onclick={toggle}>
    <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.3">
      <rect x="2" y="3" width="12" height="11" rx="1.5" />
      <path d="M2 6.5h12M5 1.5v3M11 1.5v3" />
    </svg>
  </button>
  {#if error}<span class="err" role="alert">Enter a date like {dateExample()}.</span>{/if}
  {#if open}
    <div class="pop" role="dialog" aria-label="Calendar">
      <div class="nav">
        <button type="button" aria-label="Previous month" onclick={() => (shown = addMonths(shown, -1))}>«</button>
        <span>{monthLabel(shown)}</span>
        <button type="button" aria-label="Next month" onclick={() => (shown = addMonths(shown, 1))}>»</button>
      </div>
      <div class="grid">
        {#each ["S", "M", "T", "W", "T", "F", "S"] as d, i (i)}<span class="dow">{d}</span>{/each}
        {#each monthGrid(shown) as day (day)}
          <button
            type="button"
            class="day"
            class:other={day.slice(0, 7) !== shown.slice(0, 7)}
            class:sel={day === value}
            class:today={day === today}
            aria-label={displayDate(day)}
            aria-pressed={day === value}
            onclick={() => pick(day)}
          >{Number(day.slice(8))}</button>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .date {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
  }
  .cal {
    display: inline-flex;
    padding: 0.2rem 0.3rem;
  }
  .pop {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 20;
    margin-top: 0.2rem;
    padding: 0.4rem;
    background: var(--bg, #fff);
    border: 1px solid rgba(128, 128, 128, 0.6);
    border-radius: 6px;
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.25);
  }
  .nav {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.25rem;
    font-weight: 600;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(7, 2rem);
    gap: 1px;
    text-align: center;
  }
  .dow {
    opacity: 0.7;
    font-size: 0.8em;
  }
  .day {
    padding: 0.2rem 0;
    border: 1px solid transparent;
    background: none;
    cursor: pointer;
    font: inherit;
    color: inherit;
  }
  .day:hover {
    background: rgba(128, 128, 128, 0.25);
  }
  .day.other {
    opacity: 0.45;
  }
  /* Selected and today are told apart by shape, not by color. */
  .day.sel {
    font-weight: 700;
    background: rgba(128, 128, 128, 0.35);
    text-decoration: underline;
  }
  .day.today {
    border-color: currentColor;
  }
  .err {
    position: absolute;
    top: 100%;
    left: 0;
    white-space: nowrap;
    font-size: 0.85em;
    color: var(--bad, #a83200);
  }
</style>
