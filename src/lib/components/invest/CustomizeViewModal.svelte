<script lang="ts">
  // Customize the selected Investments view: its name, columns, accounts,
  // and equities. Nothing changes until OK.
  import Modal from "../Modal.svelte";
  import {
    availableColumns,
    columnLabel,
    moveItem,
    orderedAccounts,
    toggle,
    type ColumnId,
    type ViewDef,
  } from "../../invest/views";
  import { investState } from "../../state/invest.svelte";
  import { investViewState } from "../../state/investview.svelte";
  import { listsState } from "../../state/lists.svelte";

  let { view, onsave, onclose }: { view: ViewDef; onsave: (v: ViewDef) => void; onclose: () => void } = $props();

  type Tab = "columns" | "accounts" | "equities";
  const TABS: { id: Tab; label: string }[] = [
    { id: "columns", label: "Columns" },
    { id: "accounts", label: "Accounts" },
    { id: "equities", label: "Equities" },
  ];

  const copy = (v: ViewDef): ViewDef => ({
    ...v,
    columns: [...v.columns],
    accountOrder: [...v.accountOrder],
    hiddenAccounts: [...v.hiddenAccounts],
    hiddenSecurities: [...v.hiddenSecurities],
  });

  // svelte-ignore state_referenced_locally
  let draft = $state<ViewDef>(copy(view));
  let tab = $state<Tab>("columns");
  let left = $state<ColumnId | null>(null);
  let right = $state<ColumnId | null>(null);
  let account = $state<number | null>(null);

  const rest = $derived(availableColumns(draft.columns));
  const accounts = $derived(orderedAccounts(draft, investViewState.available));
  const equities = $derived(
    [...investState.securities].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase())),
  );

  function add() {
    if (left === null) return;
    draft.columns = [...draft.columns, left];
    right = left;
    left = null;
  }
  function remove() {
    if (right === null) return;
    draft.columns = draft.columns.filter((c) => c !== right);
    left = right;
    right = null;
  }
  function moveColumn(delta: -1 | 1) {
    if (right === null) return;
    draft.columns = moveItem(draft.columns, draft.columns.indexOf(right), delta).list;
  }
  function moveAccount(delta: -1 | 1) {
    if (account === null) return;
    // The order saved is the whole list as shown, hidden accounts too.
    draft.accountOrder = moveItem(accounts, accounts.indexOf(account), delta).list;
  }
  function reset() {
    draft = copy(investViewState.fresh());
    left = right = account = null;
  }
  function save() {
    const name = draft.name.trim() || view.name;
    // Fix the account order as shown, so new accounts stay last.
    onsave({ ...draft, name, accountOrder: accounts });
  }
</script>

<Modal title="Customize view" {onclose} wide>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      save();
    }}
  >
    <label class="name">
      Name of view
      <input bind:value={draft.name} maxlength="40" />
    </label>

    <div class="tabs" role="tablist">
      {#each TABS as t (t.id)}
        <button type="button" role="tab" aria-selected={tab === t.id} class:on={tab === t.id} onclick={() => (tab = t.id)}>{t.label}</button>
      {/each}
    </div>

    {#if tab === "columns"}
      <div class="cols">
        <div>
          <div class="cap" id="avail-cap">Available columns</div>
          <ul aria-labelledby="avail-cap">
            {#each rest as c (c)}
              <li><button type="button" class:sel={left === c} aria-pressed={left === c} onclick={() => ((left = c), (right = null))}>{columnLabel(c)}</button></li>
            {:else}
              <li class="none">None</li>
            {/each}
          </ul>
        </div>
        <div class="mid">
          <button type="button" onclick={add} disabled={left === null}>Add&gt;&gt;</button>
          <button type="button" onclick={remove} disabled={right === null}>&lt;&lt;Remove</button>
        </div>
        <div>
          <div class="cap" id="shown-cap">Displayed columns (Name always shows)</div>
          <ul aria-labelledby="shown-cap">
            {#each draft.columns as c (c)}
              <li><button type="button" class:sel={right === c} aria-pressed={right === c} onclick={() => ((right = c), (left = null))}>{columnLabel(c)}</button></li>
            {:else}
              <li class="none">None</li>
            {/each}
          </ul>
          <div class="move">
            <button type="button" onclick={() => moveColumn(-1)} disabled={right === null || draft.columns.indexOf(right) === 0}>Move Up</button>
            <button type="button" onclick={() => moveColumn(1)} disabled={right === null || draft.columns.indexOf(right) === draft.columns.length - 1}>Move Down</button>
          </div>
        </div>
      </div>
    {:else if tab === "accounts"}
      <p class="hint">Checked accounts show, in this order.</p>
      <ul class="checks">
        {#each accounts as id (id)}
          <li class:sel={account === id}>
            <label>
              <input type="checkbox" checked={!draft.hiddenAccounts.includes(id)} onchange={(e) => (draft.hiddenAccounts = toggle(draft.hiddenAccounts, id, e.currentTarget.checked))} />
              {listsState.account(id)?.name ?? `#${id}`}
            </label>
            <button type="button" class="pick" aria-pressed={account === id} onclick={() => (account = id)}>Select</button>
          </li>
        {:else}
          <li class="none">No investment accounts.</li>
        {/each}
      </ul>
      <div class="move">
        <button type="button" onclick={() => moveAccount(-1)} disabled={account === null || accounts.indexOf(account) === 0}>Move Up</button>
        <button type="button" onclick={() => moveAccount(1)} disabled={account === null || accounts.indexOf(account) === accounts.length - 1}>Move Down</button>
      </div>
    {:else}
      <p class="hint">Checked equities show.</p>
      <ul class="checks">
        {#each equities as s (s.id)}
          <li>
            <label>
              <input type="checkbox" checked={!draft.hiddenSecurities.includes(s.id)} onchange={(e) => (draft.hiddenSecurities = toggle(draft.hiddenSecurities, s.id, e.currentTarget.checked))} />
              {s.name}{s.ticker ? ` (${s.ticker})` : ""}
            </label>
          </li>
        {:else}
          <li class="none">No equities.</li>
        {/each}
      </ul>
    {/if}

    <div class="buttons">
      <button type="button" onclick={reset}>Reset View</button>
      <span class="grow"></span>
      <button type="button" onclick={onclose}>Cancel</button>
      <button type="submit">OK</button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: grid;
    gap: 0.6rem;
  }
  .name {
    display: grid;
    gap: 0.15rem;
    font-size: var(--fs-register);
    max-width: 24rem;
  }
  .tabs {
    display: flex;
    gap: 0.25rem;
  }
  .on {
    font-weight: 700;
    text-decoration: underline;
  }
  .cols {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    gap: 0.75rem;
    align-items: start;
  }
  .cap {
    font-size: var(--fs-register);
    margin-bottom: 0.2rem;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0.2rem;
    min-height: 9rem;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .cols li button {
    width: 100%;
    text-align: left;
    background: none;
    border: 1px solid transparent;
    font: inherit;
    color: inherit;
    cursor: pointer;
    padding: 0.15rem 0.4rem;
  }
  li button.sel {
    background: var(--active-bg);
    font-weight: 700;
    border-color: currentColor;
  }
  .mid {
    display: grid;
    gap: 0.4rem;
    align-self: center;
  }
  .move {
    display: flex;
    gap: 0.4rem;
    margin-top: 0.4rem;
  }
  .checks {
    max-height: 18rem;
    overflow: auto;
  }
  .checks li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.1rem 0.3rem;
  }
  .checks li.sel {
    background: var(--active-bg);
  }
  .none,
  .hint {
    opacity: 0.7;
    margin: 0;
  }
  .buttons {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .grow {
    flex: 1;
  }
</style>
