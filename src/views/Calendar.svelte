<script lang="ts">
  import { call, commands } from "../lib/api";
  import { addMonths, displayDate, monthGrid, monthLabel, monthStart, weekdayNames } from "../lib/format/date";
  import { bookSettings } from "../lib/state/booksettings.svelte";
  import { formatMoney } from "../lib/format/money";
  import AccountBalance from "../lib/components/AccountBalance.svelte";
  import DayModal from "../lib/components/DayModal.svelte";
  import { listsState } from "../lib/state/lists.svelte";
  import { scheduleState } from "../lib/state/schedule.svelte";
  import { mergeItems, type CalItem } from "../lib/calendar/items";
  import type { CalendarTxn, DayBalance } from "../lib/types/bindings";

  /** First day of the week (SET-030). */
  const weekStart = $derived<0 | 1>(bookSettings.value.week_start === "monday" ? 1 : 0);
  const WEEK = $derived(weekdayNames(weekStart));

  let month = $state(monthStart(listsState.today || "2026-01-01"));
  let account = $state("");
  let showDone = $state(false);
  let projection = $state(false);
  let selected = $state<string | null>(null);
  /** The day whose transactions are open in a dialog, and the item
   * clicked to open it (`schedule-nominal`). */
  let dayOpen = $state<{ day: string; pick: string | null } | null>(null);
  let items = $state<CalItem[]>([]);
  let balances = $state<DayBalance[]>([]);
  let error = $state<string | null>(null);
  let seq = 0;

  const grid = $derived(monthGrid(month, weekStart));
  const byDay = $derived.by(() => {
    const m = new Map<string, CalItem[]>();
    for (const v of items) m.set(v.date, [...(m.get(v.date) ?? []), v]);
    return m;
  });
  const balanceByDay = $derived(new Map(balances.map((b) => [b.date, b.balance])));
  const keyOf = (v: CalItem) => v.key;
  /** An investment account's balance is not projected. */
  const investmentSelected = $derived(account !== "" && listsState.account(Number(account))?.investment != null);

  async function load() {
    const mine = ++seq;
    const [from, to] = [grid[0], grid[41]];
    const filter = account === "" ? null : [Number(account)];
    try {
      // "Show entered" adds what is done: entered and skipped occurrences,
      // and register transactions not from a schedule (CAL-020).
      const [occ, txns, proj] = await Promise.all([
        call(commands.calendarOccurrences(from, to, filter, showDone)),
        showDone ? call(commands.calendarTransactions(from, to, filter)) : Promise.resolve([] as CalendarTxn[]),
        projection && account !== "" && !investmentSelected
          ? call(commands.calendarProjection(Number(account), from, to))
          : Promise.resolve([] as DayBalance[]),
      ]);
      if (mine !== seq) return;
      items = mergeItems(occ, txns);
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

  /** A transaction was clicked: its day's dialog opens with it chosen. */
  function open(v: CalItem, e: Event) {
    e.stopPropagation();
    selected = v.date;
    dayOpen = { day: v.date, pick: keyOf(v) };
  }

  /** A click on a day's blank space: its dialog, nothing chosen. */
  function openDay(day: string, e: Event) {
    if ((e.target as HTMLElement).closest(".chip, .more")) return;
    selected = day;
    dayOpen = { day, pick: null };
  }

  const inMonth = (d: string) => d.slice(0, 7) === month.slice(0, 7);
  const payeeOf = (v: CalItem) =>
    v.payee === null ? "(no payee)" : (listsState.payee(v.payee)?.name ?? "");
</script>

<section class="cal">
  <header>
    <button type="button" aria-label="Previous month" onclick={() => (month = addMonths(month, -1))}>‹</button>
    <strong class="label">{monthLabel(month)}</strong>
    <button type="button" aria-label="Next month" onclick={() => (month = addMonths(month, 1))}>›</button>
    <button type="button" onclick={() => (month = monthStart(listsState.today))}>Today</button>
    <label>
      Account
      <select bind:value={account} aria-label="Calendar account">
        <option value="">All accounts</option>
        {#each listsState.accounts.filter((a) => a.status === "open") as a (a.id)}
          <option value={String(a.id)}>{a.name}</option>
        {/each}
      </select>
    </label>
    <label><input type="checkbox" bind:checked={showDone} /> Show entered transactions</label>
    <label title={investmentSelected ? "An investment account's balance is not projected" : "Choose an account first"}>
      <input type="checkbox" bind:checked={projection} disabled={account === "" || investmentSelected} /> Projected balance
    </label>
  </header>
  {#if error}<p class="err">{error}</p>{/if}

  <div class="grid sheet" role="grid" aria-label="Month">
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
        onclick={(e) => openDay(day, e)}
        onkeydown={(e) => (e.key === "Enter" || e.key === " ") && openDay(day, e)}
      >
        <span class="num">{Number(day.slice(8, 10))}</span>
        <!-- The selected day shows every item; others show three and "+n more". -->
        {#each day === selected ? list : list.slice(0, 3) as v (v.key)}
          <span
            class="chip"
            class:overdue={v.overdue}
            class:done={v.status !== "pending"}
            class:skipped={v.status === "skipped"}
            class:go={v.actionable && v.status === "pending"}
            role="button"
            tabindex="0"
            title={`${payeeOf(v)} ${formatMoney(v.amount)} (click for this day's transactions)`}
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
</section>

{#if dayOpen}
  <DayModal
    day={dayOpen.day}
    accounts={account === "" ? null : [Number(account)]}
    pick={dayOpen.pick}
    onclose={() => (dayOpen = null)}
  />
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
  .label {
    min-width: 9rem;
    text-align: center;
  }
  /* The month is a sheet (base.css): its edge is the outer border; the
     cells draw the lines between them. */
  /* The month fills the window: six weeks share its height. */
  .grid {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    grid-template-rows: auto repeat(6, minmax(5.5rem, 1fr));
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  /* Last column (every 7th cell, headings included) and last week. */
  .grid > :nth-child(7n) {
    border-right: none;
  }
  .grid > :nth-last-child(-n + 7) {
    border-bottom: none;
  }
  .dow {
    grid-row: 1;
    text-align: center;
    font-weight: 600;
    padding: 0.2rem;
    border-right: 1px solid var(--line-soft);
    border-bottom: 1px solid var(--line-soft);
  }
  .day {
    border-right: 1px solid var(--line-soft);
    border-bottom: 1px solid var(--line-soft);
    padding: 0.15rem 0.25rem;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    cursor: pointer;
    overflow: hidden;
    font-size: var(--fs-register);
  }
  .day.other {
    opacity: 0.45;
  }
  .day.today {
    background: var(--today-bg);
  }
  .day.sel {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
  .num {
    font-weight: 600;
  }
  .chip {
    font-size: var(--fs-register);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chip:hover {
    text-decoration: underline;
  }
  .chip.overdue {
    color: var(--bad);
  }
  .chip .od {
    font-size: var(--fs-small);
    text-transform: uppercase;
  }
  /* Done items fade; only a skipped one is struck through. */
  .chip.done {
    opacity: 0.6;
  }
  .chip.skipped {
    text-decoration: line-through;
  }
  .more {
    all: unset;
    cursor: pointer;
    text-decoration: underline;
    font-size: var(--fs-register);
  }
  .more:focus-visible {
    outline: 2px solid var(--focus-ring);
  }
  .proj {
    margin-top: auto;
    text-align: right;
    font-variant-numeric: tabular-nums;
    opacity: 0.8;
  }
  .err {
    color: var(--bad);
  }
</style>
