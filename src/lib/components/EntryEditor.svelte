<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { DECLINED, call, commands, withConfirmation } from "../api";
  import { displayDate } from "../format/date";
  import { blockNonAmountChar, formatMoney, isZeroMoney, sanitizeAmountInput } from "../format/money";
  import {
    SPLIT,
    applyQuickFill,
    buildEntry,
    dateFieldKey,
    draftFromEntry,
    emptySplit,
    isBlank,
    newDraft,
    setAmountField,
    splitParts,
    type Draft,
  } from "../register/draft";
  import { confirmState } from "../state/confirm.svelte";
  import { listsState } from "../state/lists.svelte";
  import { registerState } from "../state/register.svelte";
  import { scheduleState } from "../state/schedule.svelte";
  import { selectOnFocus } from "../ui/selectOnFocus";
  import type { AccountId, Payee } from "../types/bindings";
  import TargetCombo from "./TargetCombo.svelte";

  /** `null` = the new-entry row; otherwise edit this transaction in place. */
  let {
    txn = null,
    account,
    ondone,
  }: { txn?: number | null; account: AccountId; ondone?: (saved: boolean) => void } = $props();

  let d = $state<Draft>(newDraft(listsState.today));
  let error = $state<string | null>(null);
  let busy = $state(false);
  let status = $state<string | null>(null);
  let suggestions = $state<Payee[]>([]);
  let remainder = $state<string | null>(null);
  let dateInput: HTMLInputElement;
  let seq = 0;
  let formEl: HTMLFormElement | undefined;
  /** The draft as loaded, to tell an untouched edit from a changed one. */
  let original = "";

  const listId = `payees-${Math.random().toString(36).slice(2)}`;
  const isSplit = $derived(d.category === SPLIT);
  /** Kind for a category typed here that does not exist yet. */
  const newKind = $derived(d.deposit.trim() ? "income" : "expense");
  /** The scheduled occurrence this new entry will record as entered. */
  let occ = $state<{ schedule: number; due: string } | null>(null);

  // A scheduled occurrence sent here to be entered: take it as the draft
  // and put the cursor on the amount (REC-110).
  $effect(() => {
    const p = registerState.prefill;
    if (txn !== null || !p || p.entry.account !== account) return;
    registerState.prefill = null;
    untrack(() => void applyPrefill(p));
  });

  async function applyPrefill(p: NonNullable<typeof registerState.prefill>) {
    const name = p.entry.payee ? (listsState.payee(p.entry.payee)?.name ?? "") : "";
    d = draftFromEntry(p.entry, name);
    occ = { schedule: p.schedule, due: p.due };
    error = null;
    status = "Scheduled transaction: change anything needed (usually the amount), then Enter.";
    await tick();
    const amount = formEl?.querySelector<HTMLInputElement>(d.deposit.trim() ? ".c-dep" : ".c-pay");
    amount?.focus();
    amount?.select();
  }

  onMount(async () => {
    if (txn !== null) {
      try {
        const entry = await call(commands.entryGet(txn, account));
        const name = entry.payee ? (listsState.payee(entry.payee)?.name ?? "") : "";
        d = draftFromEntry(entry, name);
        original = JSON.stringify($state.snapshot(d));
      } catch (e) {
        error = e instanceof Error ? e.message : String(e);
      }
    }
    dateInput?.focus();
    dateInput?.select();
    if (txn !== null) formEl?.scrollIntoView?.({ block: "nearest" });
  });

  // Live split remainder from Rust (TXN-020); latest answer wins.
  $effect(() => {
    if (!isSplit) return;
    const parts = splitParts($state.snapshot(d) as Draft);
    const mine = ++seq;
    if (parts === null) {
      remainder = null;
      return;
    }
    void call(commands.splitRemainder(parts.total, parts.parts)).then(
      (r) => {
        if (mine === seq) remainder = r;
      },
      () => {
        if (mine === seq) remainder = null;
      },
    );
  });

  function onDateKey(e: KeyboardEvent) {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const next = dateFieldKey(e.key, d.date, listsState.today);
    if (next !== null) {
      e.preventDefault();
      d.date = next;
    }
  }

  async function onPayeeInput() {
    const text = d.payee.trim();
    if (!text) {
      suggestions = [];
      return;
    }
    try {
      suggestions = await call(commands.payeeSearch(text, 10));
    } catch {
      suggestions = [];
    }
  }

  /** QuickFill (PAY-020): only when adding, and only into empty fields. */
  function onPayeeChange() {
    if (txn !== null) return;
    const name = d.payee.trim().toLowerCase();
    const p =
      suggestions.find((x) => x.name.toLowerCase() === name) ??
      listsState.payees.find((x) => x.name.toLowerCase() === name);
    if (p) d = applyQuickFill($state.snapshot(d) as Draft, p);
  }

  /** Hidden payees are not offered; prefix match, as `payee_search` does. */
  function uniquePayeeMatch(): Payee | null {
    const t = d.payee.trim().toLowerCase();
    if (!t) return null;
    const hits = listsState.payees.filter(
      (p) => !p.hidden && p.name.toLowerCase().startsWith(t),
    );
    return hits.length === 1 ? hits[0] : null;
  }

  /**
   * Tab or Enter on a payee with exactly one memorized match takes it
   * (QuickFill included) and moves on to the next field (PAY-020).
   */
  function onPayeeKey(e: KeyboardEvent) {
    if ((e.key !== "Tab" && e.key !== "Enter") || e.shiftKey) return;
    const p = uniquePayeeMatch();
    if (!p) return;
    d.payee = p.name;
    if (txn === null) d = applyQuickFill($state.snapshot(d) as Draft, p);
    if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      (e.currentTarget as HTMLElement)
        .closest("form")
        ?.querySelector<HTMLElement>(".c-pay")
        ?.focus();
    }
  }

  function addSplit() {
    d.splits = [...d.splits, emptySplit()];
  }

  /** Amount fields take digits, commas, and one decimal point only. */
  function amountField(field: "payment" | "deposit") {
    return (e: Event & { currentTarget: HTMLInputElement }) => {
      const clean = sanitizeAmountInput(e.currentTarget.value);
      if (clean !== e.currentTarget.value) e.currentTarget.value = clean;
      d = setAmountField(d, field, clean);
    };
  }

  function splitAmountInput(i: number) {
    return (e: Event & { currentTarget: HTMLInputElement }) => {
      const clean = sanitizeAmountInput(e.currentTarget.value);
      if (clean !== e.currentTarget.value) e.currentTarget.value = clean;
      d.splits[i].amount = clean;
    };
  }

  /**
   * An empty split amount is offered what is still unassigned, as the
   * magnitude (TXN-020): the whole total on the first line, then what is
   * left after each completed line. Rust computes it; nothing is added up
   * here. Nothing is offered once the split is complete or over-allocated.
   */
  async function prefill(i: number) {
    const s = d.splits[i];
    if (!s || s.amount.trim() !== "") return;
    const parts = splitParts($state.snapshot(d) as Draft);
    if (parts === null) return;
    try {
      const r = await call(commands.splitRemainder(parts.total, parts.parts));
      if (isZeroMoney(r) || r.startsWith("-") !== parts.total.startsWith("-")) return;
      if (d.splits[i] && d.splits[i].amount.trim() === "") {
        d.splits[i].amount = r.replace(/^-/, "");
      }
    } catch {
      /* the remainder line shows the problem */
    }
  }

  /** Unassigned amount left, in the total's direction. */
  const unassigned = $derived(
    remainder !== null &&
      !isZeroMoney(remainder) &&
      remainder.startsWith("-") === (splitParts($state.snapshot(d) as Draft)?.total.startsWith("-") ?? false),
  );

  function focusSplit(i: number) {
    formEl
      ?.querySelector<HTMLElement>(`[aria-label="Split ${i + 1} category"]`)
      ?.focus();
  }

  /** Tab out of the last line while amount is left over: open a new line. */
  async function onSplitMemoKey(e: KeyboardEvent, i: number) {
    if (e.key !== "Tab" || e.shiftKey || i !== d.splits.length - 1 || !unassigned) return;
    e.preventDefault();
    addSplit();
    await tick();
    focusSplit(i + 1);
  }

  async function onCategoryChange() {
    if (d.category === SPLIT && d.splits.length === 0) {
      d.splits = [emptySplit(), emptySplit()];
      await tick();
      await prefill(0);
      focusSplit(0);
    }
  }

  async function save(e?: Event) {
    e?.preventDefault();
    error = null;
    if (txn === null && isBlank(d)) return; // Enter on an empty new row
    // Enter on an untouched edit writes nothing; it just moves on.
    if (txn !== null && original !== "" && JSON.stringify($state.snapshot(d)) === original) {
      ondone?.(true);
      return;
    }
    status = null;
    const built = buildEntry($state.snapshot(d) as Draft, account, listsState.today);
    if (!built.ok) {
      error = built.error;
      return;
    }
    if (isSplit && (remainder === null || !isZeroMoney(remainder))) {
      error = `Split lines must add up to the total${
        remainder !== null ? ` (remainder ${formatMoney(remainder)})` : ""
      }.`;
      return;
    }
    busy = true;
    try {
      const name = built.payeeName;
      let savedId: number = txn ?? -1;
      if (txn === null && occ) {
        const entered = await call(
          commands.scheduleEnter(
            occ.schedule,
            occ.due,
            { date: null, amount: null, entry: built.entry },
            name,
            false,
          ),
        );
        savedId = entered.txn;
        occ = null;
        await scheduleState.changed();
      } else if (txn === null) {
        savedId = await call(commands.entryCreate(built.entry, name));
      } else {
        const id = txn;
        const done = await withConfirmation(
          (c) => commands.entryUpdate(id, built.entry, name, c),
          confirmState.ask,
        );
        if (done === DECLINED) return;
      }
      if (name) await listsState.loadPayees();
      registerState.selected = savedId;
      registerState.reveal = savedId;
      await registerState.refresh();
      if (txn === null) {
        // Ready for the next one; keep the date for fast entry.
        status = `Saved ${displayDate(built.entry.date)} ${name} ${formatMoney(built.entry.amount)}`;
        d = newDraft(listsState.today, built.entry.date);
        remainder = null;
        await tick();
        // Keep the saved row fully in view, even as this row's height
        // changes (the grid re-applies it on resize).
        registerState.reveal = savedId;
        dateInput?.focus();
        dateInput?.select();
      }
      ondone?.(true);
    } catch (err) {
      error =
        (err instanceof Error ? err.message : String(err)) || "Save failed.";
    } finally {
      busy = false;
    }
  }

  function cancel() {
    error = null;
    occ = null;
    status = null;
    if (txn === null) {
      d = newDraft(listsState.today);
      remainder = null;
    }
    ondone?.(false);
  }

  function onkeydown(e: KeyboardEvent) {
    // A text input submits the form on Enter by itself; a select does not.
    if (e.key === "Enter" && e.target instanceof HTMLSelectElement) {
      e.preventDefault();
      void save();
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      cancel();
    }
  }

  /** Focus the date field (used by the grid's "new entry" shortcut). */
  export function focus() {
    dateInput?.focus();
    dateInput?.select();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<form class="entry" class:editing={txn !== null} onsubmit={save} {onkeydown} bind:this={formEl}>
  {#if isSplit && !registerState.descending}{@render splitPanel()}{/if}
  <div class="cells">
    <input class="c-date" aria-label="Date" bind:this={dateInput} bind:value={d.date} onkeydown={onDateKey} onblur={() => (d.date = d.date.trim())} />
    <input class="c-num" aria-label="Num" bind:value={d.check_num} />
    <input class="c-payee" aria-label="Payee" list={listId} autocomplete="off" bind:value={d.payee} oninput={onPayeeInput} onchange={onPayeeChange} onkeydown={onPayeeKey} />
    <datalist id={listId}>{#each suggestions as p (p.id)}<option value={p.name}></option>{/each}</datalist>
    <input class="c-pay num" aria-label="Payment" inputmode="decimal" use:selectOnFocus value={d.payment} onbeforeinput={blockNonAmountChar} oninput={amountField("payment")} />
    <input class="c-dep num" aria-label="Deposit" inputmode="decimal" use:selectOnFocus value={d.deposit} onbeforeinput={blockNonAmountChar} oninput={amountField("deposit")} />
    <div class="c-cat">
      <TargetCombo bind:value={d.category} excludeAccount={account} allowSplit newKind={newKind} onchange={onCategoryChange} />
    </div>
    <select class="c-tag" aria-label="Tag" bind:value={d.tag}>
      <option value="">—</option>
      {#each listsState.tags.filter((t) => !t.hidden || d.tag === String(t.id)) as t (t.id)}
        <option value={String(t.id)}>{t.name}</option>
      {/each}
    </select>
    <input class="c-memo" aria-label="Memo" bind:value={d.memo} />
    <span class="c-clr">{d.cleared === "cleared" ? "c" : d.cleared === "reconciled" ? "R" : ""}</span>
    <span class="c-actions">
      <button type="submit" disabled={busy}>{txn === null ? "Enter" : "Save"}</button>
      <button type="button" onclick={cancel}>Cancel</button>
    </span>
  </div>

  {#snippet splitPanel()}
    <div class="split" role="group" aria-label="Split lines">
      <div class="split-title">
        Split of {d.payment.trim() ? `payment ${d.payment}` : d.deposit.trim() ? `deposit ${d.deposit}` : "the total above"}: give each part a category or transfer account and an amount. Amounts are positive; each new line offers what is left.
      </div>
      <div class="split-line split-head" aria-hidden="true">
        <span>Category or transfer account</span><span class="num">Amount</span><span>Memo</span><span></span>
      </div>
      {#each d.splits as s, i (i)}
        <div class="split-line" role="group" aria-label={`Split line ${i + 1}`} onfocusin={() => prefill(i)}>
          <TargetCombo bind:value={s.target} excludeAccount={account} newKind={newKind} label={`Split ${i + 1} category`} />
          <input aria-label={`Split ${i + 1} amount`} class="num" inputmode="decimal" use:selectOnFocus value={s.amount} onbeforeinput={blockNonAmountChar} oninput={splitAmountInput(i)} />
          <input aria-label={`Split ${i + 1} memo`} bind:value={s.memo} onkeydown={(e) => onSplitMemoKey(e, i)} />
          <button type="button" tabindex="-1" aria-label={`Remove split ${i + 1}`} onclick={() => (d.splits = d.splits.filter((_, j) => j !== i))}>×</button>
        </div>
      {/each}
      <div class="split-foot">
        <button type="button" onclick={addSplit}>Add line</button>
        <span class="rem" class:ok={remainder !== null && isZeroMoney(remainder)}>
          Remainder: {remainder === null ? "—" : `${isZeroMoney(remainder) ? "✓" : "✗"} ${formatMoney(remainder)}`}
        </span>
      </div>
    </div>
  {/snippet}

  {#if isSplit && registerState.descending}{@render splitPanel()}{/if}
  {#if txn === null}
    <!-- Always one line tall, so a message appearing never resizes the row. -->
    <div class="msg" class:err={error} class:ok={!error && status} role={error ? "alert" : "status"}>{error ?? status ?? ""}</div>
  {:else if error}
    <div class="err" role="alert">{error}</div>
  {/if}
</form>

<style>
  .entry {
    border-top: 1px solid rgba(128, 128, 128, 0.4);
    padding: 0.25rem 0;
  }
  .entry.editing {
    background: rgba(80, 130, 220, 0.12);
  }
  .cells {
    display: grid;
    grid-template-columns: var(--cols);
    gap: 2px;
    align-items: center;
    padding-right: var(--gap-r, 0.5rem);
  }
  input,
  select {
    min-width: 0;
    width: 100%;
    box-sizing: border-box;
    font: inherit;
  }
  .num {
    text-align: right;
  }
  .c-actions {
    display: flex;
    gap: 2px;
  }
  .split {
    margin: 0.25rem 0 0.25rem 4rem;
    display: grid;
    gap: 2px;
    max-width: 44rem;
  }
  .split-line {
    display: grid;
    grid-template-columns: 2fr 8rem 2fr 2rem;
    gap: 2px;
  }
  .split-title {
    font-size: 0.85em;
    opacity: 0.85;
  }
  .split-head {
    font-size: 0.8em;
    font-weight: 600;
    opacity: 0.85;
  }
  .split-foot {
    display: flex;
    gap: 1rem;
    align-items: center;
  }
  .rem {
    color: var(--bad, #a83200);
  }
  .rem.ok {
    color: var(--good, #005a9c);
  }
  .err {
    color: var(--bad, #a83200);
    padding: 0.15rem 0.25rem;
  }
  .msg {
    min-height: 1.4em;
    padding: 0.15rem 0.25rem;
  }
  .ok {
    color: var(--good, #005a9c);
    padding: 0.15rem 0.25rem;
  }
</style>
