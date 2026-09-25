<script lang="ts">
  import { untrack } from "svelte";
  import { call, commands } from "../api";
  import { displayDate, parseDate } from "../format/date";
  import {
    blockNonAmountChar,
    formatMoney,
    negateMoney,
    parseMoney,
    sanitizeAmountInput,
  } from "../format/money";
  import { listsState } from "../state/lists.svelte";
  import { scheduleState } from "../state/schedule.svelte";
  import type { OccurrenceView } from "../types/bindings";
  import AccountBalance from "./AccountBalance.svelte";

  let { view, onchange }: { view: OccurrenceView; onchange?: () => void } = $props();

  let panel = $state(false);
  let error = $state<string | null>(null);
  let busy = $state(false);

  const init = untrack(() => view);
  let dateText = $state(displayDate(init.date));
  let amountText = $state(formatMoney(init.amount.replace(/^-/, "")));

  const schedule = $derived(scheduleState.row(view.schedule));
  const single = $derived((schedule?.schedule.fields.lines.length ?? 1) === 1);
  const negative = $derived(view.amount.startsWith("-"));
  const payee = $derived(view.payee === null ? "" : (listsState.payee(view.payee)?.name ?? ""));
  const who = $derived(`${payee || "(no payee)"} · ${listsState.account(view.account)?.name ?? ""}`);
  const done = $derived(view.status !== "pending");

  /** The edits typed in the panel, or an error message. */
  function edits(): { date: string | null; amount: string | null } | string {
    const date = parseDate(dateText, listsState.today);
    if (date === null) return "Enter a valid date.";
    let amount: string | null = null;
    if (single) {
      const mag = parseMoney(amountText);
      if (mag === null || amountText.trim() === "") return "Enter a valid amount.";
      const signed = negative ? negateMoney(mag.replace(/^-/, "")) : mag.replace(/^-/, "");
      if (signed !== view.amount) amount = signed;
    }
    return { date: date !== view.date ? date : null, amount };
  }

  async function run(action: () => Promise<unknown>) {
    busy = true;
    error = null;
    try {
      await action();
      panel = false;
      await scheduleState.changed();
      onchange?.();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  /** Entering always goes through the register, where the whole
   * transaction can be edited first (REC-110). */
  function enter() {
    error = null;
    void scheduleState.enterOccurrence(view).catch((e) => {
      error = e instanceof Error ? e.message : String(e);
    });
  }

  function setOverride() {
    const e = edits();
    if (typeof e === "string") {
      error = e;
      return;
    }
    void run(() => call(commands.scheduleOverride(view.schedule, view.nominal, e.date, e.amount)));
  }

  function clearOverride() {
    void run(() => call(commands.scheduleOverride(view.schedule, view.nominal, null, null)));
  }

  function skip() {
    void run(() => call(commands.scheduleSkip(view.schedule, view.nominal)));
  }

  function onAmount(e: Event) {
    amountText = sanitizeAmountInput((e.target as HTMLInputElement).value);
  }
</script>

<div class="occ" class:overdue={view.overdue} class:done>
  <div class="main">
    <span class="date">{displayDate(view.date)}</span>
    <span class="who" title={who}>{who}</span>
    <span class="amt"><AccountBalance amount={view.amount} /></span>
    <span class="tags">
      {#if view.overdue}<b class="badge bad">Overdue</b>{/if}
      {#if view.estimated}<b class="badge">Estimate</b>{/if}
      {#if view.overridden}<b class="badge">This one edited</b>{/if}
      {#if view.mode === "auto"}<b class="badge">Auto</b>{/if}
      {#if view.status === "entered"}<b class="badge">Entered</b>{/if}
      {#if view.status === "skipped"}<b class="badge">Skipped</b>{/if}
    </span>
    {#if !done}
      <span class="actions">
        {#if view.actionable}
          <button type="button" disabled={busy} onclick={enter}>Enter</button>
          <button type="button" disabled={busy} onclick={skip}>Skip</button>
        {/if}
        <button type="button" disabled={busy} onclick={() => (panel = !panel)}>Edit…</button>
      </span>
    {/if}
  </div>
  {#if panel}
    <div class="panel">
      <label>
        Date
        <input aria-label="Date" bind:value={dateText} />
      </label>
      <label>
        Amount
        <input
          aria-label="Amount"
          inputmode="decimal"
          value={amountText}
          disabled={!single}
         
          onbeforeinput={blockNonAmountChar}
          oninput={onAmount}
        />
      </label>
      {#if !single}<span class="note">A split's amount is edited in the register after entering.</span>{/if}
      <button type="button" disabled={busy} onclick={setOverride}>Set for this occurrence only</button>
      {#if view.overridden}
        <button type="button" disabled={busy} onclick={clearOverride}>Undo the change</button>
      {/if}
      <button type="button" onclick={() => (panel = false)}>Close</button>
    </div>
  {/if}
  {#if error}<p class="err" role="alert">{error}</p>{/if}
</div>

<style>
  .occ {
    border-bottom: 1px solid rgba(128, 128, 128, 0.25);
    padding: 0.3rem 0;
  }
  .occ.done {
    opacity: 0.7;
  }
  .main {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem 0.75rem;
    align-items: baseline;
  }
  .date {
    font-variant-numeric: tabular-nums;
    min-width: 6rem;
  }
  /* One line: smaller text, cut with "…" (the full text is the tooltip). */
  .who {
    flex: 1;
    min-width: 8rem;
    font-size: 0.9em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .amt {
    min-width: 6rem;
    text-align: right;
  }
  .badge {
    font-size: 0.75em;
    border: 1px solid rgba(128, 128, 128, 0.6);
    border-radius: 3px;
    padding: 0 0.3rem;
    margin-right: 0.2rem;
    font-weight: 600;
  }
  .badge.bad {
    color: var(--bad, #a83200);
    border-color: var(--bad, #a83200);
  }
  .panel {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: end;
    padding: 0.4rem 0 0.2rem;
  }
  .panel label {
    display: grid;
    gap: 0.1rem;
    font-size: 0.85em;
  }
  .panel input {
    width: 8rem;
  }
  .note {
    opacity: 0.7;
    font-size: 0.85em;
  }
  .err {
    color: var(--bad, #a83200);
    margin: 0.2rem 0 0;
  }
</style>
