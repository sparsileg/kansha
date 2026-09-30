<script lang="ts">
  // Choose which dashboard cards show and in what order (DSH-040). Nothing
  // changes until OK.
  import Modal from "./Modal.svelte";
  import { moveItem } from "../invest/views";
  import {
    defaultLayout,
    orderedCards,
    setCardShown,
    type CardId,
    type CardLayout,
  } from "../dashboard/cards";

  let { layout, onsave, onclose }: { layout: CardLayout; onsave: (l: CardLayout) => void; onclose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let draft = $state<CardLayout>({ order: [...layout.order], hidden: [...layout.hidden] });
  let picked = $state<CardId | null>(null);

  const cards = $derived(orderedCards(draft));
  const at = $derived(picked === null ? -1 : draft.order.indexOf(picked));

  function move(delta: -1 | 1) {
    if (picked === null) return;
    draft = { ...draft, order: moveItem(draft.order, at, delta).list };
  }
</script>

<Modal title="Customize dashboard" {onclose}>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      onsave(draft);
    }}
  >
    <p class="hint">Checked cards show, in this order.</p>
    <ul>
      {#each cards as c (c.id)}
        <li class:sel={picked === c.id}>
          <label>
            <input
              type="checkbox"
              checked={!draft.hidden.includes(c.id)}
              onchange={(e) => (draft = setCardShown(draft, c.id, e.currentTarget.checked))}
            />
            {c.label}
          </label>
          <button type="button" aria-pressed={picked === c.id} aria-label={`Select ${c.label}`} onclick={() => (picked = c.id)}>Select</button>
        </li>
      {/each}
    </ul>
    <div class="move">
      <button type="button" onclick={() => move(-1)} disabled={picked === null || at === 0}>Move Up</button>
      <button type="button" onclick={() => move(1)} disabled={picked === null || at === draft.order.length - 1}>Move Down</button>
    </div>
    <div class="buttons">
      <button type="button" onclick={() => ((draft = defaultLayout()), (picked = null))}>Reset</button>
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
  ul {
    list-style: none;
    margin: 0;
    padding: 0.2rem;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.75rem;
    padding: 0.1rem 0.3rem;
  }
  li.sel {
    background: var(--active-bg);
  }
  .hint {
    opacity: 0.7;
    margin: 0;
  }
  .move,
  .buttons {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .grow {
    flex: 1;
  }
</style>
