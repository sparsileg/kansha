<script lang="ts">
  // A budget's Graph View (BUD-060): one bar per row for the period
  // chosen. Every figure comes from Rust (`budget_graph`), in whole
  // dollars; this file only lays them out.
  import type { Snippet } from "svelte";
  import { call, commands } from "../../api";
  import { budgetState, type BudgetWin } from "../../state/budget.svelte";
  import { statusState } from "../../state/status.svelte";
  import { parseMoney, formatMoneyWhole } from "../../format/money";
  import type { GraphAt, GraphPeriod, GraphRow, GraphView } from "../../types/bindings";
  import BudgetSummary from "./BudgetSummary.svelte";

  let {
    w,
    ensureYear,
    drill,
    viewPick,
    gear,
  }: {
    w: BudgetWin;
    /** Whether `year` is held, after asking to create it (BUD-050). */
    ensureYear: (year: number, held: number[]) => Promise<boolean>;
    drill: (r: GraphRow, from: string, to: string) => void;
    viewPick: Snippet;
    gear: Snippet;
  } = $props();

  const PERIODS: [GraphPeriod, string][] = [
    ["monthly", "Monthly"],
    ["quarterly", "Quarterly"],
    ["yearly", "Yearly"],
    ["quarter_to_date", "Quarter to date"],
    ["year_to_date", "Year to date"],
    ["last_3_months", "Last 3 months"],
    ["last_6_months", "Last 6 months"],
  ];

  let view = $state<GraphView | null>(null);
  let error = $state<string | null>(null);
  let collapsed = $state<Record<string, boolean>>({});

  async function load() {
    try {
      view = await call(commands.budgetGraph(w.budget, w.period, w.at));
      error = null;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
  $effect(() => {
    void w.period;
    void w.at;
    void budgetState.version;
    void load();
  });

  /** A new period starts on the current one. */
  function pick(p: GraphPeriod) {
    w.at = null;
    w.period = p;
  }
  async function go(at: GraphAt | null) {
    if (!at || !view) return;
    if (await ensureYear(at.year, view.budget.years)) w.at = at;
  }
  async function createMissing(year: number) {
    if (view && (await ensureYear(year, view.budget.years))) await load();
  }

  // ---- The period's budget (hover makes it an edit box) ----
  let focused = $state<string | null>(null);
  let typed = $state("");

  function onfocus(key: string, r: GraphRow) {
    focused = key;
    typed = r.entered.replace(/\.00$/, "");
  }
  async function commit(key: string, r: GraphRow) {
    if (focused !== key || r.category === null || !view) return;
    focused = null;
    const amount = typed.trim() === "" ? "0.00" : parseMoney(typed);
    if (amount === null) {
      statusState.show(`“${typed}” is not an amount.`, "alert");
      return;
    }
    if (amount === r.entered) return;
    try {
      await call(commands.budgetSetPeriod(w.budget, r.category, w.period, view.at, amount));
      await load();
    } catch (e) {
      statusState.show(e instanceof Error ? e.message : String(e), "alert");
    }
  }
  function onkey(e: KeyboardEvent) {
    const el = e.currentTarget as HTMLInputElement;
    if (e.key === "Enter") el.blur();
    else if (e.key === "Escape") {
      focused = null;
      el.blur();
    }
  }

  interface Shown {
    row: GraphRow;
    kind: "group" | "row" | "other" | "child" | "total";
    key: string;
  }
  const rows = $derived.by<Shown[]>(() => {
    if (!view) return [];
    const out: Shown[] = [];
    for (const g of view.groups) {
      out.push({ row: g.total, kind: "group", key: g.kind });
      if (collapsed[g.kind]) continue;
      for (const r of g.rows) out.push({ row: r, kind: "row", key: `c${r.category}` });
      if (g.other) {
        const k = `other-${g.kind}`;
        out.push({ row: g.other, kind: "other", key: k });
        // Everything Else starts closed; ▸ lists its categories.
        if (collapsed[k] === false) {
          for (const c of g.other.children) out.push({ row: c, kind: "child", key: `x${c.category}` });
        }
      }
    }
    out.push({ row: view.total, kind: "total", key: "total" });
    return out;
  });
</script>

<div class="controls">
  {@render viewPick()}
  <label
    >Period
    <select value={w.period} onchange={(e) => pick(e.currentTarget.value as GraphPeriod)}>
      {#each PERIODS as [v, label] (v)}<option value={v}>{label}</option>{/each}
    </select>
  </label>
  {#if view?.at}
    <span class="nav">
      <button type="button" aria-label="Previous period" disabled={!view.prev} onclick={() => void go(view!.prev)}>◀</button>
      <strong>{view.title}</strong>
      <button type="button" aria-label="Next period" disabled={!view.next} onclick={() => void go(view!.next)}>▶</button>
    </span>
  {/if}
  {@render gear()}
</div>

{#if error}<p class="err" role="alert">⚠ {error}</p>{/if}
{#if view}
  {#if view.missing_year !== null}
    <p class="note">
      ⚠ No budget for {view.missing_year}: budgets are 0.
      <button type="button" onclick={() => void createMissing(view!.missing_year!)}>Create {view.missing_year}…</button>
    </p>
  {/if}

  <BudgetSummary summary={view.summary} />

  <div class="graph-wrap">
    <table class="graph">
      <thead>
        <tr>
          <th class="label">Category</th>
          <th class="barcol">Actual</th>
          <th class="num">Balance</th>
          <th class="num">Budget</th>
        </tr>
      </thead>
      <tbody>
        {#each rows as s (s.key)}
          {@const r = s.row}
          <tr class={s.kind}>
            <th scope="row" class="label">
              {#if s.kind === "group"}
                <button type="button" class="fold" aria-expanded={!collapsed[s.key]} onclick={() => (collapsed[s.key] = !collapsed[s.key])}>
                  {collapsed[s.key] ? "▶" : "▼"}
                </button>
                {r.name.toUpperCase()}
              {:else if s.kind === "other"}
                <button
                  type="button"
                  class="fold"
                  aria-expanded={collapsed[s.key] === false}
                  onclick={() => (collapsed[s.key] = collapsed[s.key] === false)}
                >
                  {collapsed[s.key] === false ? "▼" : "▶"}
                </button>
                {r.name}
              {:else if s.kind === "total"}
                Total (Income − Expenses)
              {:else}
                <span class="path">{r.prefix}</span><b>{r.name}</b>
              {/if}
            </th>
            <td class="barcol">
              <div class="track">
                {#if r.bar}
                  {@const b = r.bar}
                  {#if b.split === null}
                    <div class="fill" style:width="{b.fill / 10}%"></div>
                  {:else}
                    <!-- Over: the whole bar striped, the part past the budget lighter. -->
                    <div class="fill over" style:width="{b.split / 10}%"></div>
                    <div class="fill over past" style:left="{b.split / 10}%" style:width="{100 - b.split / 10}%"></div>
                  {/if}
                {/if}
                <span class="actual">
                  {#if s.kind === "row" || s.kind === "other" || s.kind === "child"}
                    <button type="button" class="fig" title="Show the transactions" onclick={() => drill(r, view!.from, view!.to)}
                      >{formatMoneyWhole(r.figures.actual)}</button>
                  {:else}
                    {formatMoneyWhole(r.figures.actual)}
                  {/if}
                </span>
              </div>
            </td>
            <td class="num">
              {#if r.standing === "left"}
                {formatMoneyWhole(r.amount)} left
              {:else}
                <span class="bad">⚠ {formatMoneyWhole(r.amount)} {r.standing}</span>
              {/if}
            </td>
            <td class="num">
              {#if s.kind === "row" && view.editable}
                <input
                  class="cell"
                  inputmode="decimal"
                  aria-label="{r.prefix}{r.name} budget for the period"
                  value={focused === s.key ? typed : formatMoneyWhole(r.figures.budget)}
                  oninput={(e) => (typed = e.currentTarget.value)}
                  onfocus={() => onfocus(s.key, r)}
                  onblur={() => void commit(s.key, r)}
                  onkeydown={onkey}
                />
              {:else if s.kind !== "other" && s.kind !== "child"}
                {formatMoneyWhole(r.figures.budget)}
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<style>
  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 1.25rem;
    align-items: center;
  }
  .controls label,
  .nav {
    display: inline-flex;
    gap: 0.4rem;
    align-items: center;
  }
  .err,
  .bad {
    color: var(--bad);
    font-weight: 700;
  }
  .err,
  .note {
    margin: 0;
  }
  .graph-wrap {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .graph {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--fs-register);
  }
  .graph th,
  .graph td {
    padding: 0.15rem 0.45rem;
    white-space: nowrap;
  }
  thead th {
    position: sticky;
    top: 0;
    background: var(--bg);
    border-bottom: 1px solid var(--line);
    z-index: 2;
  }
  tbody tr:nth-child(even) {
    background: var(--row-alt);
  }
  tbody .label {
    font-weight: normal;
  }
  .label {
    text-align: left;
  }
  .num {
    text-align: right;
  }
  .barcol {
    width: 55%;
    text-align: left;
  }
  .group th,
  .group td,
  .total th,
  .total td {
    font-weight: 700;
  }
  tbody tr.group:not(:first-child) th,
  tbody tr.group:not(:first-child) td {
    border-top: 1px solid var(--line);
  }
  .total th,
  .total td {
    border-top: 2px solid var(--line);
  }
  .row .label,
  .other .label {
    padding-left: 1.6rem;
  }
  .child .label {
    padding-left: 3rem;
  }
  .path {
    opacity: 0.85;
  }
  .track {
    position: relative;
    height: 1.3em;
    border: 1px solid var(--line);
    border-radius: 3px;
    overflow: hidden;
  }
  .fill {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    background: var(--good);
  }
  .fill.over {
    background: repeating-linear-gradient(45deg, var(--bad) 0 6px, transparent 6px 9px), var(--bad);
  }
  .fill.past {
    opacity: 0.5;
  }
  /* On its own background, so it reads on any bar in any theme. */
  .actual {
    position: absolute;
    left: 0.3rem;
    top: 50%;
    transform: translateY(-50%);
    padding: 0 0.25rem;
    background: var(--bg);
    border-radius: 2px;
  }
  .fold,
  .fig {
    border: none;
    background: none;
    padding: 0 0.2rem;
    cursor: pointer;
    font: inherit;
    color: inherit;
  }
  .fig {
    padding: 0;
    text-decoration: underline dotted;
  }
  .fig:hover {
    text-decoration: underline;
  }
  .cell {
    width: 6em;
    text-align: right;
    font: inherit;
    color: inherit;
    border: 1px solid transparent;
    background: transparent;
    padding: 0 0.15rem;
  }
  .cell:hover,
  .cell:focus {
    border-color: var(--line);
    background: var(--bg);
  }
</style>
