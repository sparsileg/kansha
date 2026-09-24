<script lang="ts">
  import { displayDate, parseDate } from "../format/date";
  import { listsState } from "../state/lists.svelte";
  import { registerState } from "../state/register.svelte";
  import type { Cleared } from "../types/bindings";

  let from = $state("");
  let to = $state("");

  // Reflect a cleared or replaced filter set back into the inputs.
  $effect(() => {
    const f = registerState.filters;
    from = f.date_from ? displayDate(f.date_from) : "";
    to = f.date_to ? displayDate(f.date_to) : "";
  });

  const fromBad = $derived(from.trim() !== "" && parseDate(from, listsState.today) === null);
  const toBad = $derived(to.trim() !== "" && parseDate(to, listsState.today) === null);

  function setDate(which: "date_from" | "date_to", v: string) {
    const iso = v.trim() === "" ? null : parseDate(v, listsState.today);
    if (v.trim() !== "" && iso === null) return;
    void registerState.setFilters({ [which]: iso });
  }

  const num = (v: string) => (v === "" ? null : Number(v));
</script>

<div class="filters" role="search" aria-label="Register filters">
  <label class="date-l">From <input class="date" class:bad={fromBad} bind:value={from} placeholder="MM/DD/YYYY" onchange={() => setDate("date_from", from)} /></label>
  <label class="date-l">To <input class="date" class:bad={toBad} bind:value={to} placeholder="MM/DD/YYYY" onchange={() => setDate("date_to", to)} /></label>
  <label>
    Payee
    <select value={registerState.filters.payee ?? ""} onchange={(e) => registerState.setFilters({ payee: num(e.currentTarget.value) })}>
      <option value="">All</option>
      {#each listsState.payees as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
    </select>
  </label>
  <label>
    Category
    <select value={registerState.filters.category ?? ""} onchange={(e) => registerState.setFilters({ category: num(e.currentTarget.value) })}>
      <option value="">All</option>
      {#each listsState.categories as c (c.id)}<option value={c.id}>{listsState.categoryPath(c.id)}</option>{/each}
    </select>
  </label>
  <label>
    Tag
    <select value={registerState.filters.tag ?? ""} onchange={(e) => registerState.setFilters({ tag: num(e.currentTarget.value) })}>
      <option value="">All</option>
      {#each listsState.tags as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
    </select>
  </label>
  <label>
    Cleared
    <select value={registerState.filters.cleared ?? ""} onchange={(e) => registerState.setFilters({ cleared: (e.currentTarget.value || null) as Cleared | null })}>
      <option value="">All</option>
      <option value="unmarked">Unmarked</option>
      <option value="cleared">Cleared</option>
      <option value="reconciled">Reconciled</option>
    </select>
  </label>
  <button type="button" disabled={!registerState.filtered} onclick={() => registerState.clearFilters()}>Clear all</button>
</div>

<style>
  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 0.75rem;
    align-items: end;
    padding: 0.25rem 0;
  }
  /* The boxes share the row and grow with it, so they follow the
     register's width when the account list is closed or opened. */
  label {
    display: flex;
    flex-direction: column;
    flex: 1 1 9rem;
    min-width: 7rem;
    font-size: 0.8em;
  }
  label.date-l {
    flex-basis: 7.5rem;
  }
  input,
  select {
    width: 100%;
    box-sizing: border-box;
    min-width: 0;
  }
  .bad {
    /* Dashed as well as colored: not a hue-only cue. */
    outline: 3px dashed var(--bad, #a83200);
  }
</style>
