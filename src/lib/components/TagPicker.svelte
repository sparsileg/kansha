<script lang="ts">
  import { call, commands } from "../api";
  import { listsState } from "../state/lists.svelte";

  /**
   * A tag dropdown that can also make a tag: "New tag…" at the end swaps
   * it for a text box; Enter creates the tag and picks it, Esc or leaving
   * the box goes back to the dropdown. `value` is a tag id as text, or ""
   * for none.
   */
  let {
    value = $bindable(),
    label = "Tag",
    class: className = "",
  }: { value: string; label?: string; class?: string } = $props();

  const NEW = "__new__";
  let adding = $state(false);
  let name = $state("");
  let error = $state<string | null>(null);
  let box = $state<HTMLInputElement>();

  function onSelect(e: Event & { currentTarget: HTMLSelectElement }) {
    if (e.currentTarget.value !== NEW) {
      value = e.currentTarget.value;
      return;
    }
    // Put the dropdown back on the current tag while the box is open.
    e.currentTarget.value = value;
    adding = true;
    name = "";
    error = null;
    queueMicrotask(() => box?.focus());
  }

  function cancel() {
    adding = false;
    error = null;
  }

  async function create() {
    const n = name.trim();
    if (n === "") return cancel();
    // An existing tag of that name is just picked.
    const have = listsState.tags.find((t) => t.name.toLowerCase() === n.toLowerCase());
    try {
      const tag = have ?? (await call(commands.tagCreate({ name: n, hidden: false })));
      if (!have) await listsState.loadAll();
      value = String(tag.id);
      adding = false;
      error = null;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      // Not the form's Enter (save).
      e.preventDefault();
      e.stopPropagation();
      void create();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      cancel();
    }
  }
</script>

{#if adding}
  <input
    bind:this={box}
    class={className}
    aria-label={`New ${label.toLowerCase()} name`}
    placeholder="New tag"
    title={error ?? "Enter creates the tag; Esc cancels"}
    class:bad={error !== null}
    bind:value={name}
    onkeydown={onKey}
    onblur={cancel}
  />
{:else}
  <select class={className} aria-label={label} value={value} onchange={onSelect}>
    <option value="">—</option>
    {#each listsState.tags.filter((t) => !t.hidden || String(t.id) === value) as t (t.id)}
      <option value={String(t.id)}>{t.name}</option>
    {/each}
    <option value={NEW}>New tag…</option>
  </select>
{/if}

<style>
  input,
  select {
    min-width: 0;
    width: 100%;
    box-sizing: border-box;
    font: inherit;
  }
  .bad {
    outline: 1px solid var(--bad);
  }
</style>
