<script lang="ts">
  // A calendar day's transactions: scheduled ones, open and done (entered
  // or skipped), and register transactions that did not come from a
  // schedule (CAL-020), whatever the calendar's "show entered" box says.
  // Pick one, then Enter it (in its register, REC-110), Edit it, or Skip
  // it; or start a new schedule on this day.
  import { untrack } from "svelte";
  import { call, commands } from "../api";
  import { displayDate } from "../format/date";
  import { dialogState } from "../state/dialogs.svelte";
  import { listsState } from "../state/lists.svelte";
  import { registerState } from "../state/register.svelte";
  import { scheduleState } from "../state/schedule.svelte";
  import { viewState } from "../state/view.svelte";
  import { mergeItems, type CalItem } from "../calendar/items";
  import type { AccountId } from "../types/bindings";
  import AccountBalance from "./AccountBalance.svelte";
  import Modal from "./Modal.svelte";

  let {
    day,
    accounts = null,
    pick = null,
    onclose,
  }: {
    day: string;
    /** The calendar's account filter; `null` is every account. */
    accounts?: AccountId[] | null;
    /** The item clicked (`schedule-nominal`), selected at first. */
    pick?: string | null;
    onclose: () => void;
  } = $props();

  const keyOf = (v: CalItem) => v.key;

  let items = $state<CalItem[]>([]);
  let selected = $state<string | null>(untrack(() => pick));
  let loaded = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function load() {
    try {
      const [occ, txns] = await Promise.all([
        call(commands.calendarOccurrences(day, day, accounts, true)),
        call(commands.calendarTransactions(day, day, accounts)),
      ]);
      items = mergeItems(occ, txns);
      if (!items.some((v) => keyOf(v) === selected)) selected = items[0] ? keyOf(items[0]) : null;
      error = null;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loaded = true;
    }
  }
  void load();

  const sel = $derived(items.find((v) => keyOf(v) === selected));
  const canAct = $derived(sel?.occ != null && sel.status === "pending" && sel.actionable);
  const why = $derived(
    sel === undefined
      ? "Choose a transaction first"
      : sel.status === "posted"
        ? "Not a scheduled transaction"
        : sel.status !== "pending"
          ? `Already ${sel.status}`
          : "Only the schedule's next transaction can be entered or skipped",
  );

  const payeeOf = (v: CalItem) =>
    v.payee === null ? "(no payee)" : (listsState.payee(v.payee)?.name ?? "");
  const statusOf = (v: CalItem) =>
    v.status === "posted"
      ? "Register"
      : v.status === "entered"
        ? "Entered"
        : v.status === "skipped"
          ? "Skipped"
          : v.overdue
            ? "Overdue"
            : "Due";

  function enter() {
    if (!sel?.occ || !canAct) return;
    // Props are gone once closed: act first.
    void scheduleState.enterOccurrence(sel.occ);
    onclose();
  }

  /** One with a transaction (entered, or a register one) opens it in its
   * register; otherwise the schedule. */
  async function edit() {
    if (!sel) return;
    const v = sel;
    if (v.status !== "pending" && v.status !== "skipped" && v.txn !== null) {
      onclose();
      viewState.navigate("account");
      await registerState.goToTransaction(v.account, v.txn);
      return;
    }
    if (!v.occ) return;
    const row = scheduleState.row(v.occ.schedule);
    if (!row) {
      error = "This schedule no longer exists.";
      return;
    }
    dialogState.editSchedule(v.occ.schedule, row.schedule.fields);
    onclose();
  }

  async function skip() {
    if (!sel?.occ || !canAct) return;
    busy = true;
    try {
      await call(commands.scheduleSkip(sel.occ.schedule, sel.occ.nominal));
      await scheduleState.changed();
      await load();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  function newSchedule() {
    dialogState.newSchedule(day);
    onclose();
  }

  function onListKey(e: KeyboardEvent) {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    if (items.length === 0) return;
    const at = items.findIndex((v) => keyOf(v) === selected);
    const next = Math.min(Math.max(at + (e.key === "ArrowDown" ? 1 : -1), 0), items.length - 1);
    selected = keyOf(items[next]);
  }
</script>

<Modal title="Transactions: {displayDate(day)}" wide {onclose}>
  {#if error}<p class="err" role="alert">{error}</p>{/if}
  {#if loaded && items.length === 0}
    <p>No transactions on this day.</p>
  {:else}
    <div
      class="list"
      role="listbox"
      tabindex="0"
      aria-label="Transactions on {displayDate(day)}"
      aria-activedescendant={selected ? `day-${selected}` : undefined}
      onkeydown={onListKey}
    >
      {#each items as v (keyOf(v))}
        <div
          id="day-{keyOf(v)}"
          class="row"
          class:sel={keyOf(v) === selected}
          class:done={v.status !== "pending"}
          class:skipped={v.status === "skipped"}
          role="option"
          tabindex="-1"
          aria-selected={keyOf(v) === selected}
          onclick={() => (selected = keyOf(v))}
          ondblclick={() => ((selected = keyOf(v)), canAct ? enter() : void edit())}
          onkeydown={(e) => e.key === "Enter" && (canAct ? enter() : void edit())}
        >
          <span class="st">{statusOf(v)}</span>
          <span class="who">{payeeOf(v)}</span>
          <span class="acct">{listsState.account(v.account)?.name ?? ""}</span>
          <AccountBalance amount={v.amount} />
        </div>
      {/each}
    </div>
  {/if}
  <div class="buttons">
    <button type="button" onclick={newSchedule}>New Schedule</button>
    <span class="gap" aria-hidden="true"></span>
    <button type="button" disabled={!canAct || busy} title={canAct ? "" : why} onclick={enter}>Enter</button>
    <button type="button" disabled={!sel || busy} title={sel ? "" : why} onclick={() => void edit()}>Edit</button>
    <button type="button" disabled={!canAct || busy} title={canAct ? "" : why} onclick={() => void skip()}>Skip</button>
    <button type="button" onclick={onclose}>Close</button>
  </div>
</Modal>

<style>
  .list {
    display: grid;
    border: 1px solid var(--line);
    max-height: 50vh;
    overflow: auto;
    margin-bottom: 0.75rem;
  }
  .row {
    display: grid;
    grid-template-columns: 5.5rem minmax(0, 1fr) minmax(0, 12rem) auto;
    gap: 0.75rem;
    padding: 0.3rem 0.5rem;
    cursor: pointer;
    align-items: baseline;
  }
  .row + .row {
    border-top: 1px solid var(--line-soft);
  }
  /* Selection: the theme's selection colors plus a bold status word. */
  .row.sel {
    background: var(--sel-bg);
    color: var(--sel-fg);
  }
  /* Amounts keep their sign text but take the selection's text color, so
     nothing sits in a low-contrast color on the selection background. */
  .row.sel :global(*) {
    color: inherit;
  }
  .row.sel .st {
    font-weight: 700;
  }
  /* Only a skipped one is struck through; entered and register
     transactions happened. */
  .row.skipped .who {
    text-decoration: line-through;
  }
  .st {
    font-size: var(--fs-register);
    text-transform: uppercase;
  }
  .who,
  .acct {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .buttons {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  /* "New Schedule" on the left, apart from the four on the right. */
  .gap {
    flex: 1;
    min-width: 2rem;
  }
  .err {
    color: var(--bad);
  }
</style>
