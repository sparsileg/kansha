<script lang="ts">
  import { onMount, tick } from "svelte";
  import { ApiError, call, commands, withConfirmation } from "../api";
  import { formatMoney, isZeroMoney } from "../format/money";
  import {
    SPLIT,
    applyQuickFill,
    buildEntry,
    dateFieldKey,
    draftFromEntry,
    emptySplit,
    newDraft,
    setAmountField,
    splitParts,
    type Draft,
  } from "../register/draft";
  import { confirmState } from "../state/confirm.svelte";
  import { listsState } from "../state/lists.svelte";
  import { registerState } from "../state/register.svelte";
  import type { AccountId, Payee } from "../types/bindings";
  import TargetSelect from "./TargetSelect.svelte";

  /** `null` = the new-entry row; otherwise edit this transaction in place. */
  let {
    txn = null,
    account,
    ondone,
  }: { txn?: number | null; account: AccountId; ondone?: () => void } = $props();

  let d = $state<Draft>(newDraft(listsState.today));
  let error = $state<string | null>(null);
  let busy = $state(false);
  let suggestions = $state<Payee[]>([]);
  let remainder = $state<string | null>(null);
  let dateInput: HTMLInputElement;
  let seq = 0;

  const listId = `payees-${Math.random().toString(36).slice(2)}`;
  const isSplit = $derived(d.category === SPLIT);

  onMount(async () => {
    if (txn !== null) {
      try {
        const entry = await call(commands.entryGet(txn, account));
        const name = entry.payee ? (listsState.payee(entry.payee)?.name ?? "") : "";
        d = draftFromEntry(entry, name);
      } catch (e) {
        error = e instanceof Error ? e.message : String(e);
      }
    }
    dateInput?.focus();
    dateInput?.select();
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

  function addSplit() {
    d.splits = [...d.splits, emptySplit()];
  }

  function onCategoryChange() {
    if (d.category === SPLIT && d.splits.length === 0) {
      d.splits = [emptySplit(), emptySplit()];
    }
  }

  async function save(e?: Event) {
    e?.preventDefault();
    error = null;
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
      if (txn === null) {
        await call(commands.entryCreate(built.entry, name));
      } else {
        const id = txn;
        const done = await withConfirmation(
          (c) => commands.entryUpdate(id, built.entry, name, c),
          confirmState.ask,
        );
        if (done === null) return;
      }
      if (name) await listsState.loadPayees();
      await registerState.refresh();
      if (txn === null) {
        // Ready for the next one; keep the date for fast entry.
        d = newDraft(listsState.today, built.entry.date);
        remainder = null;
        await tick();
        dateInput?.focus();
        dateInput?.select();
      }
      ondone?.();
    } catch (err) {
      error = err instanceof ApiError || err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  function cancel() {
    error = null;
    if (txn === null) {
      d = newDraft(listsState.today);
      remainder = null;
    }
    ondone?.();
  }

  function onkeydown(e: KeyboardEvent) {
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
<form class="entry" class:editing={txn !== null} onsubmit={save} {onkeydown}>
  <div class="cells">
    <input class="c-date" aria-label="Date" bind:this={dateInput} bind:value={d.date} onkeydown={onDateKey} onblur={() => (d.date = d.date.trim())} />
    <input class="c-num" aria-label="Num" bind:value={d.check_num} />
    <input class="c-payee" aria-label="Payee" list={listId} autocomplete="off" bind:value={d.payee} oninput={onPayeeInput} onchange={onPayeeChange} />
    <datalist id={listId}>{#each suggestions as p (p.id)}<option value={p.name}></option>{/each}</datalist>
    <input class="c-pay num" aria-label="Payment" inputmode="decimal" value={d.payment} oninput={(e) => (d = setAmountField(d, "payment", e.currentTarget.value))} />
    <input class="c-dep num" aria-label="Deposit" inputmode="decimal" value={d.deposit} oninput={(e) => (d = setAmountField(d, "deposit", e.currentTarget.value))} />
    <div class="c-cat">
      <TargetSelect bind:value={d.category} excludeAccount={account} allowSplit onchange={onCategoryChange} />
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

  {#if isSplit}
    <div class="split" role="group" aria-label="Split lines">
      {#each d.splits as s, i (i)}
        <div class="split-line">
          <TargetSelect bind:value={s.target} excludeAccount={account} label={`Split ${i + 1} category`} />
          <input aria-label={`Split ${i + 1} amount`} class="num" inputmode="decimal" bind:value={s.amount} />
          <input aria-label={`Split ${i + 1} memo`} bind:value={s.memo} />
          <button type="button" aria-label={`Remove split ${i + 1}`} onclick={() => (d.splits = d.splits.filter((_, j) => j !== i))}>×</button>
        </div>
      {/each}
      <div class="split-foot">
        <button type="button" onclick={addSplit}>Add line</button>
        <span class="rem" class:ok={remainder !== null && isZeroMoney(remainder)}>
          Remainder: {remainder === null ? "—" : formatMoney(remainder)}
        </span>
      </div>
    </div>
  {/if}
  {#if error}<div class="err" role="alert">{error}</div>{/if}
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
  .split-foot {
    display: flex;
    gap: 1rem;
    align-items: center;
  }
  .rem {
    color: #c0392b;
  }
  .rem.ok {
    color: #2e8b57;
  }
  .err {
    color: #c0392b;
    padding: 0.15rem 0.25rem;
  }
</style>
