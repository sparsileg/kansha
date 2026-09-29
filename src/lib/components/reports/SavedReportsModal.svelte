<script lang="ts">
  // Saved reports (RPT-020): open, rename, or delete one.
  import { onMount } from "svelte";
  import { call, commands } from "../../api";
  import { REPORTS } from "../../reports/meta";
  import { confirmState } from "../../state/confirm.svelte";
  import type { SavedReport } from "../../types/bindings";
  import Modal from "../Modal.svelte";

  let { onopen, onclose }: { onopen: (r: SavedReport) => void; onclose: () => void } = $props();

  let saved = $state<SavedReport[]>([]);
  let selected = $state<SavedReport | null>(null);
  let name = $state("");
  let error = $state<string | null>(null);

  async function load() {
    try {
      saved = await call(commands.savedReportList());
      error = null;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
  onMount(load);

  function pick(r: SavedReport) {
    selected = r;
    name = r.name;
  }

  async function rename() {
    if (!selected) return;
    try {
      const r = await call(commands.savedReportUpdate(selected.id, name, selected.settings));
      await load();
      pick(r);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function remove() {
    if (!selected || !(await confirmState.ask(`Delete the saved report "${selected.name}"?`))) return;
    try {
      await call(commands.savedReportDelete(selected.id));
      selected = null;
      name = "";
      await load();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<Modal title="Saved Reports" {onclose}>
  <ul class="list" aria-label="Saved reports">
    {#each saved as r (r.id)}
      <li>
        <button type="button" class:sel={selected?.id === r.id} aria-pressed={selected?.id === r.id} onclick={() => pick(r)} ondblclick={() => onopen(r)}>
          {r.name} <span class="kind">{REPORTS[r.settings.kind].name}</span>
        </button>
      </li>
    {:else}
      <li class="none">No saved reports yet. Open a report, customize it, and choose Save.</li>
    {/each}
  </ul>
  {#if selected}
    <label class="name">Name <input bind:value={name} maxlength="80" /></label>
  {/if}
  {#if error}<p class="err" role="alert">{error}</p>{/if}
  <div class="buttons">
    <button type="button" disabled={!selected} onclick={() => selected && onopen(selected)}>Open</button>
    <button type="button" disabled={!selected || !name.trim() || name === selected.name} onclick={rename}>Rename</button>
    <button type="button" disabled={!selected} onclick={remove}>Delete</button>
    <span class="grow"></span>
    <button type="button" onclick={onclose}>Close</button>
  </div>
</Modal>

<style>
  .list {
    list-style: none;
    margin: 0 0 0.6rem;
    padding: 0.2rem;
    min-height: 10rem;
    max-height: 22rem;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .list button {
    width: 100%;
    text-align: left;
    background: none;
    border: 1px solid transparent;
    font: inherit;
    color: inherit;
    padding: 0.15rem 0.4rem;
    cursor: pointer;
    display: flex;
    justify-content: space-between;
    gap: 1rem;
  }
  .list button.sel {
    background: var(--active-bg);
    border-color: currentColor;
    font-weight: 700;
  }
  .kind {
    opacity: 0.75;
    font-weight: 400;
  }
  .name {
    display: grid;
    gap: 0.15rem;
    margin-bottom: 0.5rem;
  }
  .none {
    opacity: 0.75;
    padding: 0.4rem;
  }
  .err {
    color: var(--bad);
  }
  .buttons {
    display: flex;
    gap: 0.5rem;
  }
  .grow {
    flex: 1;
  }
</style>
