<script lang="ts">
  import { untrack } from "svelte";
  import { call, commands } from "../api";
  import { blockNonAmountChar, sanitizeAmountInput } from "../format/money";
  import {
    PRESETS,
    WEEKDAYS,
    buildFields,
    draftFromFields,
    emptyLine,
    newScheduleDraft,
    type ScheduleDraft,
  } from "../schedule/form";
  import { confirmState } from "../state/confirm.svelte";
  import { dialogState } from "../state/dialogs.svelte";
  import { listsState } from "../state/lists.svelte";
  import { scheduleState } from "../state/schedule.svelte";
  import { selectOnFocus } from "../ui/selectOnFocus";
  import type { ScheduleFields, ScheduleId } from "../types/bindings";
  import Modal from "./Modal.svelte";
  import TargetCombo from "./TargetCombo.svelte";

  let {
    id,
    fields,
    start,
  }: { id: ScheduleId | null; fields: ScheduleFields | null; start: string | null } = $props();

  const init = untrack(() => ({ id, fields, start }));
  let d = $state<ScheduleDraft>(
    init.fields
      ? draftFromFields(
          structuredClone($state.snapshot(init.fields)),
          init.fields.payee === null ? "" : (listsState.payee(init.fields.payee)?.name ?? ""),
        )
      : newScheduleDraft(null, init.start ?? listsState.today),
  );
  let error = $state<string | null>(null);
  let busy = $state(false);

  const editing = init.id !== null;
  const payeeListId = `sched-payees-${Math.random().toString(36).slice(2)}`;
  /** One line: its memo is the transaction memo, as in the register. */
  const single = $derived(d.lines.length === 1);
  const mainAccount = $derived(d.account === "" ? undefined : Number(d.account));
  const monthly = $derived(["monthly", "quarterly", "twice_yearly"].includes(d.preset));
  const usesEvery = $derived(
    ["daily", "weekly", "monthly", "last_day", "nth_weekday", "yearly"].includes(d.preset),
  );
  const everyUnit = $derived(
    { daily: "days", weekly: "weeks", yearly: "years" }[d.preset as string] ?? "months",
  );

  function onAmount(line: { amount: string }, e: Event) {
    line.amount = sanitizeAmountInput((e.target as HTMLInputElement).value);
  }

  async function save() {
    const built = buildFields(d, listsState.today);
    if (!built.ok) {
      error = built.error;
      return;
    }
    busy = true;
    error = null;
    try {
      if (init.id === null) await call(commands.scheduleCreate(built.fields, built.payeeName));
      else await call(commands.scheduleUpdate(init.id, built.fields, built.payeeName));
      if (built.payeeName) await listsState.loadPayees();
      await scheduleState.changed();
      dialogState.schedule = undefined;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (init.id === null) return;
    const ok = await confirmState.ask(
      "Delete this schedule? Transactions it already entered stay.",
    );
    if (!ok) return;
    try {
      await call(commands.scheduleDelete(init.id));
      await scheduleState.changed();
      dialogState.schedule = undefined;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<Modal
  title={editing ? "Edit scheduled transaction" : "New scheduled transaction"}
  wide
  onclose={() => (dialogState.schedule = undefined)}
>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void save();
    }}
  >
    <div class="grid">
      <label>
        Account
        <select bind:value={d.account}>
          <option value="">(choose)</option>
          {#each listsState.accounts.filter((a) => a.investment === null && (a.status === "open" || String(a.id) === d.account)) as a (a.id)}
            <option value={String(a.id)}>{a.name}</option>
          {/each}
        </select>
      </label>
      <label>
        Payee
        <input list={payeeListId} autocomplete="off" bind:value={d.payee} placeholder="Choose or type a new payee" />
        <datalist id={payeeListId}>
          {#each listsState.payees.filter((p) => !p.hidden) as p (p.id)}<option value={p.name}></option>{/each}
        </datalist>
      </label>
      <label>
        Memo
        <input bind:value={d.memo} />
      </label>
      <label>
        Method
        <select bind:value={d.direction}>
          <option value="payment">Payment</option>
          <option value="deposit">Deposit</option>
        </select>
      </label>
    </div>

    <fieldset>
      <legend>Amount and category</legend>
      <div class="line head" aria-hidden="true">
        <span>Amount</span><span>Category or transfer</span>{#if !single}<span>Memo</span>{/if}<span>Tag</span><span></span>
      </div>
      {#each d.lines as line, i (i)}
        <div class="line" class:one={single}>
          <input
            class="amt"
            aria-label={`Line ${i + 1} amount`}
            inputmode="decimal"
            use:selectOnFocus
            value={line.amount}
            onbeforeinput={blockNonAmountChar}
            oninput={(e) => onAmount(line, e)}
          />
          <TargetCombo bind:value={line.target} excludeAccount={mainAccount} newKind={d.direction === "deposit" ? "income" : "expense"} label={`Line ${i + 1} category`} />
          {#if !single}<input aria-label={`Line ${i + 1} memo`} bind:value={line.memo} />{/if}
          <select aria-label={`Line ${i + 1} tag`} bind:value={line.tag}>
            <option value="">—</option>
            {#each listsState.tags.filter((t) => !t.hidden || String(t.id) === line.tag) as t (t.id)}
              <option value={String(t.id)}>{t.name}</option>
            {/each}
          </select>
          <span class="end">
            {#if d.lines.length > 1}
              <button type="button" aria-label={`Remove line ${i + 1}`} onclick={() => d.lines.splice(i, 1)}>×</button>
            {/if}
            {#if i === d.lines.length - 1}
              <button type="button" class="split" aria-label="Split" title="Split into another category" onclick={() => d.lines.push(emptyLine())}>
                <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M8 14V8M8 8L3 2M8 8l5-6M3 2v3M3 2h3M13 2v3M13 2h-3" /></svg>
                Split
              </button>
            {/if}
          </span>
        </div>
      {/each}
      <p class="note">The scheduled amount is the sum of the lines.</p>
    </fieldset>

    <fieldset>
      <legend>How often</legend>
      <div class="grid">
        <label>
          Frequency
          <select bind:value={d.preset}>
            {#each PRESETS as [v, label] (v)}<option value={v}>{label}</option>{/each}
          </select>
        </label>
        {#if usesEvery}
          <label>
            Every ({everyUnit})
            <input inputmode="numeric" bind:value={d.every} />
          </label>
        {/if}
        {#if monthly || d.preset === "twice_monthly"}
          <label>
            Day of month
            <input inputmode="numeric" bind:value={d.day1} />
          </label>
        {/if}
        {#if d.preset === "twice_monthly"}
          <label>
            and day
            <input inputmode="numeric" bind:value={d.day2} />
          </label>
        {/if}
        {#if d.preset === "nth_weekday"}
          <label>
            Week
            <select bind:value={d.weekOfMonth}>
              <option value="1">First</option>
              <option value="2">Second</option>
              <option value="3">Third</option>
              <option value="4">Fourth</option>
              <option value="-1">Last</option>
            </select>
          </label>
          <label>
            Weekday
            <select bind:value={d.weekday}>
              {#each WEEKDAYS as w, i (w)}<option value={String(i + 1)}>{w}</option>{/each}
            </select>
          </label>
        {/if}
        <label>
          {editing ? "Next due on or after" : "First due on or after"}
          <input bind:value={d.start} placeholder="MM/DD/YYYY" />
        </label>
        <label>
          On a weekend
          <select bind:value={d.weekendRule}>
            <option value="none">Leave as is</option>
            <option value="previous">Move to Friday</option>
            <option value="next">Move to Monday</option>
          </select>
        </label>
      </div>
    </fieldset>

    <fieldset>
      <legend>Ends</legend>
      <div class="grid">
        <label>
          End
          <select bind:value={d.endKind}>
            <option value="never">Never</option>
            <option value="on_date">On a date</option>
            <option value="after_count">After a number of occurrences</option>
          </select>
        </label>
        {#if d.endKind === "on_date"}
          <label>End date <input bind:value={d.endDate} placeholder="MM/DD/YYYY" /></label>
        {:else if d.endKind === "after_count"}
          <label># left <input inputmode="numeric" bind:value={d.count} /></label>
        {/if}
      </div>
    </fieldset>

    <fieldset>
      <legend>Entering</legend>
      <div class="grid">
        <label>
          Remind days before due
          <input inputmode="numeric" bind:value={d.remindDays} />
        </label>
        <label>
          Mode
          <select bind:value={d.mode}>
            <option value="remind">Remind me</option>
            <option value="auto">Enter automatically (flag for review)</option>
          </select>
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={d.estimated} />
          Amount is an estimate (confirm each time)
        </label>
      </div>
    </fieldset>

    {#if editing}
      <p class="note">Changes apply to this and all future occurrences. Entered ones stay as they are.</p>
    {/if}
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    <div class="buttons">
      {#if editing}<button type="button" onclick={remove}>Delete…</button>{/if}
      <span class="spacer"></span>
      <button type="submit" disabled={busy}>Save</button>
      <button type="button" onclick={() => (dialogState.schedule = undefined)}>Cancel</button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: grid;
    gap: 0.75rem;
  }
  fieldset {
    border: 1px solid rgba(128, 128, 128, 0.4);
    border-radius: 4px;
    display: grid;
    gap: 0.5rem;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(11rem, 1fr));
    gap: 0.5rem;
  }
  label {
    display: grid;
    gap: 0.15rem;
    font-size: 0.9em;
  }
  label.check {
    display: flex;
    gap: 0.4rem;
    align-items: center;
  }
  .line {
    display: grid;
    grid-template-columns: 7rem minmax(10rem, 1.4fr) minmax(6rem, 1fr) 7rem 6.5rem;
    gap: 0.4rem;
    align-items: center;
  }
  .line.one {
    grid-template-columns: 7rem minmax(10rem, 1.4fr) 7rem 6.5rem;
  }
  .line.head {
    font-size: 0.8em;
    opacity: 0.7;
  }
  .amt {
    text-align: right;
    min-width: 0;
  }
  .end {
    display: flex;
    gap: 0.25rem;
    justify-content: flex-end;
  }
  .split {
    display: inline-flex;
    gap: 0.25rem;
    align-items: center;
  }
  .buttons {
    display: flex;
    gap: 0.5rem;
  }
  .spacer {
    flex: 1;
  }
  .note {
    opacity: 0.7;
    margin: 0;
  }
  .err {
    color: var(--bad, #a83200);
    margin: 0;
  }
</style>
