<script module lang="ts">
  /** Tags being made from a name box left by a click or Tab (#67). */
  const pending = new Set<Promise<void>>();

  /** Wait for any tag still being made, so a Save sees its id. */
  export async function tagsSettled(): Promise<void> {
    while (pending.size > 0) await Promise.allSettled([...pending]);
  }
</script>

<script lang="ts">
  import { call, commands } from "../api";
  import { listsState } from "../state/lists.svelte";

  /**
   * A tag dropdown that can also make a tag: "New tag…" at the end swaps
   * it for a text box; Enter, Tab, or leaving the box creates the tag and
   * picks it (#67); Esc, or leaving it empty, goes back to the dropdown.
   * `value` is a tag id as text, or "" for none. A form's Save first
   * awaits `tagsSettled()`.
   */
  let {
    value = $bindable(),
    label = "Tag",
    empty = "—",
    class: className = "",
  }: { value: string; label?: string; empty?: string; class?: string } = $props();

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

  let creating: Promise<void> | null = null;

  /** Make the typed tag once, however many of Enter and blur ask. */
  function create(): Promise<void> {
    if (creating) return creating;
    const p = make().finally(() => {
      pending.delete(p);
      creating = null;
    });
    creating = p;
    pending.add(p);
    return p;
  }

  async function make() {
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
    onblur={() => {
      if (adding) void create();
    }}
  />
{:else}
  <select class={className} class:hint={value === "" && empty !== "—"} aria-label={label} value={value} onchange={onSelect}>
    <option value="">{empty}</option>
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
  /* An empty entry-row tag shows its name as a hint, like the text boxes. */
  select.hint {
    color: color-mix(in srgb, currentColor 55%, transparent);
  }
  select.hint option {
    color: var(--opt-fg);
  }
</style>
