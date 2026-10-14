<script lang="ts">
  // Select categories to budget… (BUD-040): income and expense
  // categories at any level, each ticked on its own (a parent's actual
  // takes in subcategories not budgeted themselves). One taken off loses
  // its amounts. Nothing changes until OK.
  import { call, commands } from "../../api";
  import { budgetState } from "../../state/budget.svelte";
  import { listsState } from "../../state/lists.svelte";
  import type { CategoryId } from "../../types/bindings";
  import Modal from "../Modal.svelte";

  let { onsaved }: { onsaved: () => void } = $props();

  const id = budgetState.choosing!;
  const budget = budgetState.list.find((b) => b.id === id);
  let chosen = $state<CategoryId[]>([...(budget?.categories ?? [])]);
  let showHidden = $state(false);
  let contains = $state("");
  let error = $state<string | null>(null);
  let saving = $state(false);

  interface Item {
    id: CategoryId;
    label: string;
    heading: string;
    hidden: boolean;
  }

  const all = $derived<Item[]>(
    (["expense", "income"] as const).flatMap((kind) =>
      listsState.categories
        .filter((c) => c.kind === kind)
        .map((c) => ({
          id: c.id,
          label: listsState.categoryPath(c.id),
          heading: kind === "expense" ? "Expenses" : "Income",
          hidden: c.hidden,
        }))
        .sort((a, b) => a.label.localeCompare(b.label)),
    ),
  );
  const shown = $derived.by(() => {
    const text = contains.trim().toLowerCase();
    return all.filter((i) => (showHidden || !i.hidden) && (!text || i.label.toLowerCase().includes(text)));
  });

  const close = () => (budgetState.choosing = null);

  function set(c: CategoryId, on: boolean) {
    chosen = on ? [...chosen.filter((x) => x !== c), c] : chosen.filter((x) => x !== c);
  }

  async function save(e: Event) {
    e.preventDefault();
    if (saving) return;
    saving = true;
    error = null;
    try {
      await call(commands.budgetCategoriesSet(id, [...chosen]));
      await budgetState.refresh();
      close();
      onsaved();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }
</script>

<Modal title="Select categories to budget" onclose={close} wide>
  <form onsubmit={save}>
    <div class="filter">
      <div>
        <ul class="list" aria-label="Categories">
          {#each shown as item, i (item.id)}
            {#if item.heading !== shown[i - 1]?.heading}<li class="head">{item.heading}</li>{/if}
            <li>
              <label class="check" class:dim={item.hidden}>
                <input type="checkbox" checked={chosen.includes(item.id)} onchange={(e) => set(item.id, e.currentTarget.checked)} />
                {item.label}
              </label>
            </li>
          {:else}
            <li class="none">None.</li>
          {/each}
        </ul>
        <div class="under">
          <label class="check"><input type="checkbox" bind:checked={showHidden} /> Show hidden</label>
          <label>Contains <input bind:value={contains} size="14" /></label>
        </div>
      </div>
      <div class="side">
        <button type="button" onclick={() => (chosen = all.map((i) => i.id))}>Select all</button>
        <button type="button" onclick={() => (chosen = [])}>Clear all</button>
      </div>
    </div>
    <p class="note">A category taken off loses its budget amounts.</p>
    {#if error}<p class="err" role="alert">⚠ {error}</p>{/if}
    <div class="buttons">
      <span class="grow"></span>
      <button type="button" onclick={close}>Cancel</button>
      <button type="submit" disabled={saving}>OK</button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: grid;
    gap: 0.6rem;
  }
  .check {
    display: inline-flex;
    gap: 0.35rem;
    align-items: center;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0.25rem;
    border: 1px solid var(--line);
    border-radius: 4px;
    height: 20rem;
    min-width: min(24rem, 90vw);
    overflow: auto;
  }
  .head {
    font-weight: 700;
    margin-top: 0.3rem;
  }
  .dim {
    font-style: italic;
    opacity: 0.75;
  }
  .filter {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 1rem;
    align-items: start;
  }
  .side {
    display: grid;
    gap: 0.4rem;
    max-width: 12rem;
  }
  .under {
    display: flex;
    gap: 1rem;
    align-items: center;
    margin-top: 0.3rem;
  }
  .under label {
    display: inline-flex;
    gap: 0.35rem;
    align-items: center;
  }
  .none,
  .note {
    opacity: 0.8;
    margin: 0;
  }
  .err {
    color: var(--bad);
    font-weight: 700;
    margin: 0;
  }
  .buttons {
    display: flex;
    gap: 0.5rem;
  }
  .grow {
    flex: 1;
  }
</style>
