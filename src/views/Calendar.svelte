<script lang="ts">
  import { call, commands } from "../lib/api";
  import { addMonths, displayDate, monthGrid, monthLabel, monthStart } from "../lib/format/date";
  import { formatMoney } from "../lib/format/money";
  import AccountBalance from "../lib/components/AccountBalance.svelte";
  import OccurrenceModal from "../lib/components/OccurrenceModal.svelte";
  import { dialogState } from "../lib/state/dialogs.svelte";
  import { listsState } from "../lib/state/lists.svelte";
  import { scheduleState } from "../lib/state/schedule.svelte";
  import type { DayBalance, OccurrenceView } from "../lib/types/bindings";

  const WEEK = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

  let month = $state(monthStart(listsState.today || "2026-01-01"));
  let account = $state("");
  let showDone = $state(false);
  let projection = $state(false);
  let selected = $state<string | null>(null);
  /** The day-panel item whose details are open (`schedule-nominal`). */
  let openKey = $state<string | null>(null);
  let items = $state<OccurrenceView[]>([]);
  let balances = $state<DayBalance[]>([]);
  let error = $state<string | null>(null);
  let seq = 0;
  let panel: HTMLElement | undefined;

  const grid = $derived(monthGrid(month));
  const byDay = $derived.by(() => {
    const m = new Map<string, OccurrenceView[]>();
    for (const v of items) m.set(v.date, [...(m.get(v.date) ?? []), v]);
    return m;
  });
  const balanceByDay = $derived(new Map(balances.map((b) => [b.date, b.balance])));
  const dayItems = $derived(selected ? (byDay.get(selected) ?? []) : []);
  const keyOf = (v: OccurrenceView) => `${v.schedule}-${v.nominal}`;
  const openItem = $derived(openKey === null ? undefined : items.find((v) => keyOf(v) === openKey));

  async function load() {
    const mine = ++seq;
    const [from, to] = [grid[0], grid[41]];
    const filter = account === "" ? null : [Number(account)];
    try {
      const [occ, proj] = await Promise.all([
        call(commands.calendarOccurrences(from, to, filter, showDone)),
        projection && account !== ""
          ? call(commands.calendarProjection(Number(account), from, to))
          : Promise.resolve([] as DayBalance[]),
      ]);
      if (mine !== seq) return;
      items = occ;
      balances = proj;
      error = null;
    } catch (e) {
      if (mine === seq) error = e instanceof Error ? e.message : String(e);
    }
  }

  // Reload when the month or a filter changes, or after any schedule change.
  $effect(() => {
    void [month, account, showDone, projection, scheduleState.rows];
    void load();
  });

  /** A reminder was clicked: the next occurrence of its schedule opens in
   * the account register, prefilled, cursor on the amount. Later ones and
   * finished ones just select their day. */
  function open(v: OccurrenceView, e: Event) {
    e.stopPropagation();
    selected = v.date;
    if (v.status !== "pending" || !v.actionable) return;
    scheduleState.enterOccurrence(v).catch((err) => {
      error = err instanceof Error ? err.message : String(err);
    });
  }

  // On a narrow window the day panel sits below the grid: bring it into view.
  $effect(() => {
    if (selected) panel?.scrollIntoView?.({ block: "nearest" });
  });

  const inMonth = (d: string) => d.slice(0, 7) === month.slice(0, 7);
  const payeeOf = (v: OccurrenceView) =>
    v.payee === null ? "(no payee)" : (listsState.payee(v.payee)?.name ?? "");
</script>

<section class="cal">
  <header>
    <h1>Calendar</h1>
    <button type="button" aria-label="Previous month" onclick={() => (month = addMonths(month, -1))}>‹</button>
    <strong class="label">{monthLabel(month)}</strong>
    <button type="button" aria-label="Next month" onclick={() => (month = addMonths(month, 1))}>›</button>
    <button type="button" onclick={() => (month = monthStart(listsState.today))}>Today</button>
    <label>
      Account
      <select bind:value={account} aria-label="Calendar account">
        <option value="">All accounts</option>
        {#each listsState.accounts.filter((a) => a.investment === null && a.status === "open") as a (a.id)}
          <option value={String(a.id)}>{a.name}</option>
        {/each}
      </select>
    </label>
    <label><input type="checkbox" bind:checked={showDone} /> Show entered and skipped</label>
    <label title="Choose an account first">
      <input type="checkbox" bind:checked={projection} disabled={account === ""} /> Projected balance
    </label>
  </header>
  {#if error}<p class="err">{error}</p>{/if}

  <div class="layout">
    <div class="grid" role="grid" aria-label="Month">
      {#each WEEK as w (w)}<div class="dow" role="columnheader">{w}</div>{/each}
      {#each grid as day (day)}
        {@const list = byDay.get(day) ?? []}
        <div
          class="day"
          class:other={!inMonth(day)}
          class:today={day === listsState.today}
          class:sel={day === selected}
          role="gridcell"
          tabindex="0"
          aria-label={displayDate(day)}
          onclick={() => (selected = day)}
          onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (selected = day)}
        >
          <span class="num">{Number(day.slice(8, 10))}</span>
          <!-- The selected day shows every item; others show three and "+n more". -->
          {#each day === selected ? list : list.slice(0, 3) as v (`${v.schedule}-${v.nominal}`)}
            <span
              class="chip"
              class:overdue={v.overdue}
              class:done={v.status !== "pending"}
              class:go={v.actionable && v.status === "pending"}
              role="button"
              tabindex="0"
              title={`${payeeOf(v)} ${formatMoney(v.amount)}${v.actionable && v.status === "pending" ? " (click to enter in the account register)" : ""}`}
              onclick={(e) => open(v, e)}
              onkeydown={(e) => (e.key === "Enter" || e.key === " ") && open(v, e)}
            >
              {#if v.overdue}<b class="od">Overdue</b>{/if}
              {payeeOf(v)} <AccountBalance amount={v.amount} />
            </span>
          {/each}
          {#if list.length > 3 && day !== selected}
            <button
              type="button"
              class="more"
              title="Show all {list.length} items for this day"
              onclick={(e) => {
                e.stopPropagation();
                selected = day;
              }}>+{list.length - 3} more</button
            >
          {/if}
          {#if projection && balanceByDay.has(day)}
            <span class="proj">{formatMoney(balanceByDay.get(day) ?? "0.00")}</span>
          {/if}
        </div>
      {/each}
    </div>

    <aside aria-label="Day" bind:this={panel}>
      {#if selected}
        <h2>{displayDate(selected)}</h2>
        {#each dayItems as v (keyOf(v))}
          {@const who = `${payeeOf(v)} · ${listsState.account(v.account)?.name ?? ""}`}
          <button type="button" class="line" title={who} onclick={() => (openKey = keyOf(v))}>
            <span class="d">{displayDate(v.date)}</span>
            <span class="who">{who}</span>
            <AccountBalance amount={v.amount} />
          </button>
        {:else}
          <p>Nothing scheduled.</p>
        {/each}
        <button type="button" onclick={() => dialogState.newSchedule(selected)}>
          New schedule on this date
        </button>
      {:else}
        <p>Click a day to see its items.</p>
      {/if}
    </aside>
  </div>
</section>

{#if openItem}
  <OccurrenceModal view={openItem} onclose={() => (openKey = null)} />
{/if}

<style>
  .cal {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    min-height: 0;
    flex: 1;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 0.75rem;
    align-items: center;
  }
  h1 {
    margin: 0;
    font-size: 1.3em;
  }
  .label {
    min-width: 9rem;
    text-align: center;
  }
  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(16rem, 22rem);
    gap: 1rem;
    min-height: 0;
    flex: 1;
    overflow: auto;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    grid-auto-rows: minmax(5.5rem, auto);
    border-top: 1px solid rgba(128, 128, 128, 0.35);
    border-left: 1px solid rgba(128, 128, 128, 0.35);
    align-self: start;
  }
  .dow {
    grid-row: 1;
    text-align: center;
    font-weight: 600;
    padding: 0.2rem;
    border-right: 1px solid rgba(128, 128, 128, 0.35);
    border-bottom: 1px solid rgba(128, 128, 128, 0.35);
  }
  .day {
    border-right: 1px solid rgba(128, 128, 128, 0.35);
    border-bottom: 1px solid rgba(128, 128, 128, 0.35);
    padding: 0.15rem 0.25rem;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    cursor: pointer;
    overflow: hidden;
    font-size: 0.85em;
  }
  .day.other {
    opacity: 0.45;
  }
  .day.today {
    background: rgba(31, 111, 235, 0.12);
  }
  .day.sel {
    outline: 2px solid #1f6feb;
    outline-offset: -2px;
  }
  .num {
    font-weight: 600;
  }
  .chip {
    font-size: 0.85em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chip.go:hover {
    text-decoration: underline;
  }
  .chip.overdue {
    color: var(--bad, #a83200);
  }
  .chip .od {
    font-size: 0.8em;
    text-transform: uppercase;
  }
  .chip.done {
    opacity: 0.6;
    text-decoration: line-through;
  }
  .more {
    all: unset;
    cursor: pointer;
    text-decoration: underline;
    font-size: 0.9em;
  }
  .more:focus-visible {
    outline: 2px solid #1f6feb;
  }
  .proj {
    margin-top: auto;
    text-align: right;
    font-variant-numeric: tabular-nums;
    opacity: 0.8;
  }
  aside {
    align-self: start;
    position: sticky;
    top: 0;
  }
  /* One line per item; the details open in a modal. */
  .line {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 0.5rem;
    width: 100%;
    text-align: left;
    font-size: 0.85em;
    margin-bottom: 0.2rem;
    align-items: baseline;
  }
  .line .d {
    font-variant-numeric: tabular-nums;
  }
  .line .who {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  aside h2 {
    margin: 0 0 0.5rem;
    font-size: 1.05em;
  }
  .err {
    color: var(--bad, #a83200);
  }
  @media (max-width: 800px) {
    .layout {
      grid-template-columns: 1fr;
    }
  }
</style>
