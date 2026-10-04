<script lang="ts">
  // Name an insight and choose its cards, in order (INS-030). Laid out as
  // Edit > Navigation Bar: what is available on the left, what is on the
  // insight on the right. Nothing changes until Save.
  import { CARDS, cardsNotIn } from "../insights/cards";
  import { addNav, moveNav, removeNav } from "../shell/navitems";
  import Modal from "./Modal.svelte";

  let {
    title,
    name: startName,
    cards: startCards,
    onsave,
    onclose,
  }: {
    title: string;
    name: string;
    cards: string[];
    /** Rejects with the reason when Rust refuses (a name in use). */
    onsave: (name: string, cards: string[]) => Promise<void>;
    onclose: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let name = $state(startName);
  // Unknown IDs (from a later release) are kept as they are.
  // svelte-ignore state_referenced_locally
  let draft = $state<string[]>([...startCards]);
  let left = $state("");
  let right = $state("");
  let error = $state<string | null>(null);
  let saving = $state(false);

  const available = $derived(cardsNotIn(draft));
  const at = $derived(draft.indexOf(right));
  const label = (id: string) => CARDS.find((c) => c.id === id)?.label ?? id;

  function add(id = left) {
    if (!id) return;
    draft = addNav(draft, id);
    right = id;
    left = "";
  }
  function remove(id = right) {
    if (!id) return;
    draft = removeNav(draft, id);
    left = id;
    right = "";
  }
  function move(delta: -1 | 1) {
    draft = moveNav(draft, right, delta);
  }
  async function save(e: Event) {
    e.preventDefault();
    if (!name.trim() || saving) return;
    saving = true;
    error = null;
    try {
      await onsave(name.trim(), draft);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }
</script>

<Modal {title} wide {onclose}>
  <form onsubmit={save}>
    <label class="name">
      <span>Name</span>
      <input type="text" bind:value={name} maxlength="60" />
    </label>
    <p class="help">Choose the cards on this insight and their order. Select a card and use the buttons.</p>
    <div class="cols">
      <div class="col">
        <label for="ins-available">Available cards</label>
        <select id="ins-available" size="10" bind:value={left} ondblclick={() => add()}>
          {#each available as c (c.id)}<option value={c.id}>{c.label}</option>{/each}
        </select>
      </div>
      <div class="mid">
        <button type="button" disabled={!left} onclick={() => add()}>Add ›</button>
        <button type="button" disabled={!right} onclick={() => remove()}>‹ Remove</button>
      </div>
      <div class="col">
        <label for="ins-shown">On this insight (in order)</label>
        <select id="ins-shown" size="10" bind:value={right} ondblclick={() => remove()}>
          {#each draft as id (id)}<option value={id}>{label(id)}</option>{/each}
        </select>
      </div>
      <div class="mid">
        <button type="button" disabled={at <= 0} aria-label="Move up (earlier)" onclick={() => move(-1)}>▲ Up</button>
        <button type="button" disabled={at < 0 || at >= draft.length - 1} aria-label="Move down (later)" onclick={() => move(1)}>▼ Down</button>
      </div>
    </div>
    {#if draft.length === 0}<p class="help">No cards yet.</p>{/if}
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    <div class="row">
      <span class="spacer"></span>
      <button type="submit" disabled={!name.trim() || saving}>Save</button>
      <button type="button" onclick={onclose}>Cancel</button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: grid;
    gap: 0.6rem;
  }
  .name {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .name input {
    flex: 1;
  }
  .help {
    margin: 0;
    opacity: 0.85;
  }
  .cols {
    display: flex;
    gap: 0.75rem;
    align-items: stretch;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    flex: 1;
    min-width: 0;
  }
  select {
    width: 100%;
    flex: 1;
  }
  .mid {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 0.4rem;
  }
  .row {
    display: flex;
    gap: 0.5rem;
  }
  .spacer {
    flex: 1;
  }
  .err {
    color: var(--bad);
    margin: 0;
  }
</style>
