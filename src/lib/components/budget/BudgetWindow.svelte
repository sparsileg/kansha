<script lang="ts">
  // A budget's window (BUD-040 … BUD-070). The window frame names it;
  // under that, the controls with the gear at the right, the summary
  // band, and the Annual View grid. Every figure comes from Rust
  // (`budget_annual`), in whole dollars; this file only lays them out.
  import { call, commands } from "../../api";
  import { budgetState, type BudgetShow } from "../../state/budget.svelte";
  import { confirmState } from "../../state/confirm.svelte";
  import { reportState } from "../../state/reports.svelte";
  import { statusState } from "../../state/status.svelte";
  import { formatMoney, isZeroMoney, parseMoney } from "../../format/money";
  import type { AnnualRow, AnnualView, BudgetFigures, CopyScope, MonthColumn } from "../../types/bindings";
  import ContextMenu, { type MenuItem } from "../ContextMenu.svelte";
  import GearButton from "../GearButton.svelte";

  let { id }: { id: number } = $props();

  // svelte-ignore state_referenced_locally
  const w = budgetState.win(id)!;
  let view = $state<AnnualView | null>(null);
  let error = $state<string | null>(null);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let collapsed = $state<Record<string, boolean>>({});
  /** The month last edited or focused in each row, for its copy icon. */
  let rowMonth = $state<Record<string, number>>({});

  const SHOW: [BudgetShow, string][] = [
    ["details", "Details"],
    ["budget", "Budget"],
    ["balance", "Balance"],
    ["actual", "Actual"],
  ];
  const FIGURE: Record<Exclude<BudgetShow, "details">, keyof BudgetFigures> = {
    budget: "budget",
    balance: "balance",
    actual: "actual",
  };

  async function load() {
    try {
      view = await call(commands.budgetAnnual(w.budget, w.year));
      error = null;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
  $effect(() => {
    void w.year;
    void budgetState.version;
    void load();
  });

  /** Whole dollars, as Rust rounded them: "1,234", "-56". */
  function dollars(m: string): string {
    return formatMoney(m).replace(/\.\d+$/, "");
  }
  const neg = (m: string) => m.startsWith("-") && !isZeroMoney(m);

  // ---- Years (BUD-050) ----
  /** The held year nearest `year`, to copy from. */
  function nearestHeld(year: number): number | null {
    const years = view?.budget.years ?? [];
    let best: number | null = null;
    for (const y of years) if (best === null || Math.abs(y - year) < Math.abs(best - year)) best = y;
    return best;
  }
  async function goYear(year: number) {
    const held = view?.budget.years ?? [];
    if (!held.includes(year)) {
      const from = nearestHeld(year);
      const choices = from === null ? ["Create it"] : [`Copy ${from}'s budget`, `Copy only the categories`];
      const pick = await confirmState.choose(`No budget for ${year}. Create one?`, choices);
      if (pick === null) return;
      try {
        await call(commands.budgetYearCreate(w.budget, year, pick === choices[0] && from !== null ? from : null));
        await budgetState.refresh();
      } catch (e) {
        statusState.show(e instanceof Error ? e.message : String(e), "alert");
        return;
      }
    }
    w.year = year;
  }

  // ---- Columns ----
  /** Past and current months show Budget, Actual, and Balance in
   * Details; future months Budget only. */
  function parts(c: MonthColumn): (keyof BudgetFigures)[] {
    if (w.show !== "details") return [FIGURE[w.show]];
    return view && c.month <= view.to_date ? ["budget", "actual", "balance"] : ["budget"];
  }
  const toDateFigure = $derived<keyof BudgetFigures>(w.show === "details" ? "balance" : FIGURE[w.show]);
  const yearFigure = $derived<keyof BudgetFigures>(w.show === "details" ? "balance" : FIGURE[w.show]);
  const PART_LABEL: Record<keyof BudgetFigures, string> = { budget: "Budget", actual: "Actual", balance: "Balance" };
  /** To Date follows the current month. */
  const showToDate = $derived(view?.current != null);

  // ---- Editing (BUD-070) ----
  const editable = $derived(view?.held === true);
  let focusKey = $state<string | null>(null);
  let typed = $state("");

  function entered(m: string): string {
    return m.replace(/\.00$/, "");
  }
  function onfocus(r: AnnualRow, i: number) {
    focusKey = `${r.category}:${i}`;
    typed = entered(r.entered[i]);
    rowMonth[String(r.category)] = i + 1;
  }
  /** The last cell save, so a cell menu acts on the amount just typed. */
  let saving: Promise<void> = Promise.resolve();
  async function commit(r: AnnualRow, i: number) {
    if (focusKey !== `${r.category}:${i}`) return;
    focusKey = null;
    if (r.category === null) return;
    const amount = typed.trim() === "" ? "0.00" : parseMoney(typed);
    if (amount === null) {
      statusState.show(`“${typed}” is not an amount.`, "alert");
      return;
    }
    if (amount === r.entered[i]) return;
    try {
      await call(commands.budgetSet(w.budget, r.category, w.year, i + 1, amount));
      await load();
    } catch (e) {
      statusState.show(e instanceof Error ? e.message : String(e), "alert");
    }
  }
  /** Enter moves down, as a spreadsheet; Tab moves right natively. */
  function onkey(e: KeyboardEvent, row: number, col: number, r: AnnualRow) {
    if (e.key === "ArrowDown" && e.altKey) {
      // The cell's ▾ menu from the keyboard.
      e.preventDefault();
      showMenu(e.currentTarget as HTMLElement, cellItems(col + 1, r));
    } else if (e.key === "Escape") {
      focusKey = null;
      (e.currentTarget as HTMLInputElement).blur();
    } else if (e.key === "Enter") {
      e.preventDefault();
      const to = document.querySelector<HTMLInputElement>(
        `input[data-cell="${row + (e.shiftKey ? -1 : 1)}:${col}"]`,
      );
      if (to) to.focus();
      else (e.currentTarget as HTMLInputElement).blur();
    }
  }

  async function copyMonth(month: number, scope: CopyScope, category: number | null) {
    try {
      await saving;
      await call(commands.budgetCopyMonth(w.budget, w.year, month, scope, category));
      await load();
    } catch (e) {
      statusState.show(e instanceof Error ? e.message : String(e), "alert");
    }
  }
  async function setAverage(month: number, category: number) {
    try {
      await saving;
      await call(commands.budgetSetAverage(w.budget, category, w.year, month));
      await load();
    } catch (e) {
      statusState.show(e instanceof Error ? e.message : String(e), "alert");
    }
  }
  const monthName = (month: number) => view?.columns[month - 1]?.long ?? "";
  /** The header Budget ▾: every category's budgets for `month`. */
  function copyItems(month: number): MenuItem[] {
    const name = monthName(month);
    return [
      { label: `Copy ${name} budgets to all future months in ${w.year}`, action: () => void copyMonth(month, "future", null) },
      { label: `Copy ${name} budgets to all past months in ${w.year}`, action: () => void copyMonth(month, "past", null) },
      { label: `Copy ${name} budgets to all months in ${w.year}`, action: () => void copyMonth(month, "all", null) },
    ];
  }
  /** A row's ⇉: one category, the month last edited or focused in it. */
  function rowItems(month: number, r: AnnualRow): MenuItem[] {
    const name = monthName(month);
    const cat = `${r.prefix}${r.name}`;
    const c = r.category!;
    return [
      { label: `Copy ${name} ${cat} budget to all future months in ${w.year}`, action: () => void copyMonth(month, "future", c) },
      { label: `Copy ${name} ${cat} budget to all past months in ${w.year}`, action: () => void copyMonth(month, "past", c) },
      { label: `Copy ${name} ${cat} budget to all months in ${w.year}`, action: () => void copyMonth(month, "all", c) },
    ];
  }
  /** A Budget cell's ▾ (BUD-070). */
  function cellItems(month: number, r: AnnualRow): MenuItem[] {
    const c = r.category!;
    return [
      { label: `Apply forward to the end of ${w.year}`, action: () => void copyMonth(month, "future", c) },
      { label: `Apply to all ${w.year}`, action: () => void copyMonth(month, "all", c) },
      {
        label: `Set ${monthName(month)} budget based on average monthly values in this category`,
        action: () => void setAverage(month, c),
      },
    ];
  }
  function showMenu(el: HTMLElement, items: MenuItem[]) {
    const r = el.getBoundingClientRect();
    menu = menu ? null : { x: r.left, y: r.bottom, items };
  }
  function openMenu(e: MouseEvent, items: MenuItem[]) {
    // The click stays here, so the menu's own window listener does not
    // close it at once (as GearButton does).
    e.stopPropagation();
    showMenu(e.currentTarget as HTMLElement, items);
  }

  // ---- Gear (BUD-040) ----
  function gear(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const name = budgetState.name(w.budget);
    menu = menu
      ? null
      : {
          x: r.right,
          y: r.bottom,
          items: [
            { label: "Select categories to budget…", action: () => (budgetState.choosing = w.budget) },
            { label: "Budget reports: Current budget (later)", action: () => {}, disabled: true },
            { label: "Budget reports: Historical budgets (later)", action: () => {}, disabled: true },
            { label: "Copy this budget…", action: () => (budgetState.naming = { mode: "copy", id: w.budget, name: `${name} copy` }) },
            { label: "Edit budget name…", action: () => (budgetState.naming = { mode: "rename", id: w.budget, name }) },
            { label: "Delete this budget…", action: () => void remove(name) },
            { label: "Create new budget…", action: () => (budgetState.naming = { mode: "create", id: null, name: "" }) },
          ],
        };
  }
  async function remove(name: string) {
    if (!(await confirmState.ask(`Delete the budget “${name}”? Its amounts go; transactions are not touched.`))) return;
    try {
      await call(commands.budgetDelete(w.budget));
      await budgetState.refresh();
      await budgetState.closeBudget(w.budget);
    } catch (e) {
      statusState.show(e instanceof Error ? e.message : String(e), "alert");
    }
  }

  // ---- Actual opens its transactions ----
  async function drill(r: AnnualRow, from: string, to: string) {
    if (r.counted.length === 0) return;
    const s = await commands.reportDefaults("itemized_categories");
    const label = r.category === null ? r.name : `${r.prefix}${r.name}`;
    await reportState.openWith({
      ...s,
      range: { preset: "custom", from, to },
      categories: r.counted,
      transfers: false,
      title: `Itemized Categories: ${label}`,
    });
  }
  function drillMonth(r: AnnualRow, i: number) {
    const c = view?.columns[i];
    if (c) void drill(r, c.from, c.to);
  }
  function drillSpan(r: AnnualRow, months: number) {
    const cols = view?.columns ?? [];
    if (months > 0) void drill(r, cols[0].from, cols[months - 1].to);
  }

  /** Rows in order, with their kind (for styling) and a number for
   * Enter's moves between budget cells. */
  interface Shown {
    row: AnnualRow;
    kind: "group" | "row" | "other" | "child" | "total";
    key: string;
    edit: number | null;
  }
  const actualOnly = (s: Shown) => s.kind === "other" || s.kind === "child";
  const rows = $derived.by<Shown[]>(() => {
    if (!view) return [];
    const out: Shown[] = [];
    let n = 0;
    for (const g of view.groups) {
      out.push({ row: g.total, kind: "group", key: g.kind, edit: null });
      if (collapsed[g.kind]) continue;
      for (const r of g.rows) out.push({ row: r, kind: "row", key: `c${r.category}`, edit: n++ });
      if (g.other) {
        const k = `other-${g.kind}`;
        out.push({ row: g.other, kind: "other", key: k, edit: null });
        // Everything Else starts closed; ▸ lists its categories.
        if (collapsed[k] === false) {
          for (const c of g.other.children) out.push({ row: c, kind: "child", key: `x${c.category}`, edit: null });
        }
      }
    }
    out.push({ row: view.total, kind: "total", key: "total", edit: null });
    return out;
  });
</script>

<div class="budget-win">
  <div class="controls">
    <label
      >View
      <select bind:value={w.view}>
        <option value="graph" disabled title="Next step">Graph View</option>
        <option value="annual">Annual View</option>
      </select>
    </label>
    <span class="year">
      <button type="button" aria-label="Previous year" onclick={() => void goYear(w.year - 1)}>◀</button>
      <strong>{w.year}</strong>
      <button type="button" aria-label="Next year" onclick={() => void goYear(w.year + 1)}>▶</button>
    </span>
    <label
      >Show
      <select bind:value={w.show}>
        {#each SHOW as [v, label] (v)}<option value={v}>{label}</option>{/each}
      </select>
    </label>
    <span class="gear"><GearButton label="Budget actions" onclick={gear} /></span>
  </div>

  {#if error}<p class="err" role="alert">⚠ {error}</p>{/if}
  {#if view}
    {#if !view.held}
      <p class="note">
        ⚠ No budget for {view.year}: budgets are 0.
        <button type="button" onclick={() => void goYear(view!.year)}>Create {view.year}…</button>
      </p>
    {/if}

    <div class="summary" aria-label="Spending summary">
      <div class="spent">
        <span class="cap">SPENDING</span>
        <strong>${dollars(view.summary.spending)}</strong>
        <span class="range">{view.summary.range}</span>
      </div>
      <div class="meter">
        <span class="budget-label">Budget: ${dollars(view.summary.budget)}</span>
        <div class="track">
          {#if view.summary.bar}
            {@const b = view.summary.bar}
            <div class="fill" class:over={b.split !== null} style:width="{b.fill / 10}%"></div>
            {#if b.split !== null}<div class="mark" style:left="{b.split / 10}%"></div>{/if}
          {/if}
        </div>
        <span class:over-text={view.summary.over}>
          {#if view.summary.over}⚠ ${dollars(view.summary.amount)} OVER{:else}${dollars(view.summary.amount)} LEFT{/if}
        </span>
      </div>
    </div>

    <div class="grid-wrap">
      <table class="grid">
        <thead>
          <tr>
            <th rowspan="2" class="label-col">Category</th>
            {#each view.columns as c (c.month)}
              <th colspan={parts(c).length} class:shade={view.current === c.month} class="month mstart">{c.short}</th>
              {#if showToDate && c.month === view.current}
                <th rowspan="2" class="todate">To Date</th>
              {/if}
            {/each}
            <th rowspan="2" class="year-col">{view.year} Summary</th>
          </tr>
          <!-- The figure under each month, in every Show choice. -->
          <tr>
            {#each view.columns as c (c.month)}
              {#each parts(c) as p, j (p)}
                <th class="sub" class:mstart={j === 0} class:shade={view.current === c.month}>
                  {PART_LABEL[p]}
                  {#if p === "budget" && editable && (w.show === "details" || w.show === "budget")}
                    <button
                      type="button"
                      class="drop"
                      aria-label="{c.long} budget copy options"
                      onclick={(e) => openMenu(e, copyItems(c.month))}>▾</button>
                  {/if}
                </th>
              {/each}
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each rows as s (s.key)}
            {@const r = s.row}
            <tr class={s.kind}>
              <th scope="row" class="label-col">
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
                  {#if s.kind === "row" && editable && (w.show === "details" || w.show === "budget")}
                    <button
                      type="button"
                      class="copy"
                      aria-label="Copy a month across this row"
                      title="Copy a month across this row"
                      onclick={(e) => openMenu(e, rowItems(rowMonth[String(r.category)] ?? view!.current ?? 1, r))}
                      >⇉</button>
                  {/if}
                {/if}
              </th>
              {#each view.columns as c, i (c.month)}
                {#each parts(c) as p, j (p)}
                  <td class="num {p}" class:mstart={j === 0} class:shade={view.current === c.month}>
                    {#if actualOnly(s) && p !== "actual"}
                      <!-- Everything Else: actual only. -->
                    {:else if p === "budget" && s.kind === "row" && editable}
                      {@const k = `${r.category}:${i}`}
                      <input
                        class="cell"
                        inputmode="decimal"
                        aria-label="{r.prefix}{r.name} {c.long} budget"
                        data-cell="{s.edit}:{i}"
                        value={focusKey === k ? typed : dollars(r.months[i].budget)}
                        oninput={(e) => (typed = e.currentTarget.value)}
                        onfocus={() => onfocus(r, i)}
                        onblur={() => (saving = commit(r, i))}
                        onkeydown={(e) => onkey(e, s.edit ?? 0, i, r)}
                      />
                      {#if w.show === "details" || w.show === "budget"}
                        <!-- Shown on hover or focus; Alt+Down opens it from the cell. -->
                        <button
                          type="button"
                          class="cell-drop"
                          tabindex="-1"
                          aria-label="{r.prefix}{r.name} {c.long} budget options"
                          onclick={(e) => openMenu(e, cellItems(c.month, r))}>▾</button>
                      {/if}
                    {:else if p === "actual" && s.kind !== "total" && s.kind !== "group"}
                      <button type="button" class="fig" onclick={() => drillMonth(r, i)} title="Show the transactions"
                        >{dollars(r.months[i].actual)}</button>
                    {:else if p === "balance"}
                      <span class:neg={neg(r.months[i].balance)}><b>{dollars(r.months[i].balance)}</b></span>
                    {:else}
                      {dollars(r.months[i][p])}
                    {/if}
                  </td>
                {/each}
                {#if showToDate && c.month === view.current}
                  <td class="num todate">
                    {#if actualOnly(s) && toDateFigure !== "actual"}
                      <!-- actual only -->
                    {:else if toDateFigure === "actual" && s.kind !== "total" && s.kind !== "group"}
                      <button type="button" class="fig" onclick={() => drillSpan(r, view!.to_date)}>{dollars(r.to_date.actual)}</button>
                    {:else}
                      <span class:neg={toDateFigure === "balance" && neg(r.to_date.balance)}
                        ><b class:plain={toDateFigure !== "balance"}>{dollars(r.to_date[toDateFigure])}</b></span>
                    {/if}
                  </td>
                {/if}
              {/each}
              <td class="num year-col">
                {#if actualOnly(s) && yearFigure !== "actual"}
                  <!-- actual only -->
                {:else if yearFigure === "actual" && s.kind !== "total" && s.kind !== "group"}
                  <button type="button" class="fig" onclick={() => drillSpan(r, 12)}>{dollars(r.year.actual)}</button>
                {:else}
                  <span class:neg={yearFigure === "balance" && neg(r.year.balance)}
                    ><b class:plain={yearFigure !== "balance"}>{dollars(r.year[yearFigure])}</b></span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
  {#if menu}
    <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
  {/if}
</div>

<style>
  .budget-win {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    gap: 0.5rem;
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 1.25rem;
    align-items: center;
  }
  .controls label {
    display: inline-flex;
    gap: 0.4rem;
    align-items: center;
  }
  .year {
    display: inline-flex;
    gap: 0.4rem;
    align-items: center;
  }
  .gear {
    margin-left: auto;
  }
  .err,
  .over-text {
    color: var(--bad);
    font-weight: 700;
  }
  .err,
  .note {
    margin: 0;
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
  .grid-wrap {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .grid {
    border-collapse: collapse;
    font-size: var(--fs-register);
    white-space: nowrap;
  }
  .grid th,
  .grid td {
    padding: 0.15rem 0.45rem;
  }
  thead tr:last-child th {
    border-bottom: 1px solid var(--line);
  }
  /* Alternate rows shaded instead of ruled. The sticky Category cell
     is opaque, so it lays the stripe over the page background. */
  tbody tr:nth-child(even) {
    background: var(--row-alt);
  }
  tbody tr:nth-child(even) .label-col {
    background: linear-gradient(var(--row-alt), var(--row-alt)), var(--bg);
  }
  tbody tr.group:not(:first-child) th,
  tbody tr.group:not(:first-child) td {
    border-top: 1px solid var(--line);
  }
  thead th {
    position: sticky;
    top: 0;
    background: var(--bg);
    z-index: 1;
  }
  thead tr:nth-child(2) th {
    top: 1.6em;
  }
  .label-col {
    text-align: left;
    position: sticky;
    left: 0;
    background: var(--bg);
    z-index: 2;
  }
  tbody .label-col {
    font-weight: normal;
  }
  .num {
    text-align: right;
  }
  .shade {
    background: var(--report-shade);
  }
  .todate,
  .year-col {
    border-left: 2px solid var(--line);
  }
  .group th,
  .group td,
  .total th,
  .total td {
    font-weight: 700;
  }
  .total th,
  .total td {
    border-top: 2px solid var(--line);
  }
  /* Categories sit under their EXPENSES / INCOME toggle; Everything
     Else's categories one step further. */
  .row .label-col,
  .other .label-col {
    padding-left: 1.6rem;
  }
  .child .label-col {
    padding-left: 3rem;
  }
  .path {
    opacity: 0.85;
  }
  .plain {
    font-weight: inherit;
  }
  .neg {
    color: var(--bad);
  }
  .fold,
  .drop,
  .copy {
    border: none;
    background: none;
    padding: 0 0.2rem;
    cursor: pointer;
    font: inherit;
    /* The text color, not WebKitGTK's pale button color. */
    color: inherit;
  }
  .copy {
    margin-left: 0.3rem;
    opacity: 0.75;
  }
  tr:hover .copy {
    opacity: 1;
  }
  .fig {
    border: none;
    background: none;
    padding: 0;
    font: inherit;
    color: inherit;
    cursor: pointer;
    text-decoration: underline dotted;
  }
  .fig:hover {
    text-decoration: underline;
  }
  .cell {
    width: 5.5em;
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
  /* A line before each month, so a Details month's Budget, Actual, and
     Balance read as one group (BUD-070). */
  .mstart {
    border-left: 1px solid var(--line);
  }
  /* A Budget cell's ▾, at its left, shown on hover or focus. */
  td.budget {
    position: relative;
  }
  .cell-drop {
    position: absolute;
    left: 0;
    top: 50%;
    transform: translateY(-50%);
    border: none;
    background: none;
    padding: 0 0.1rem;
    cursor: pointer;
    font: inherit;
    color: inherit;
    opacity: 0;
  }
  td.budget:hover .cell-drop,
  td.budget:focus-within .cell-drop {
    opacity: 1;
  }
</style>
