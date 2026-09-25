<script lang="ts">
  import { tick } from "svelte";
  import { call, commands } from "../api";
  import { SPLIT, type TargetValue } from "../register/draft";
  import { matchTargets, newCategoryPath, type TargetOption } from "../register/match";
  import { confirmState } from "../state/confirm.svelte";
  import { listsState } from "../state/lists.svelte";
  import type { AccountId } from "../types/bindings";

  /**
   * Category / transfer / split picker as a type-ahead box (REG-030).
   * Typing filters categories and accounts together; Up/Down move; Tab or
   * Enter (while the list is open) take the highlighted one; Esc closes
   * the list. With the list closed, Enter belongs to the form (save).
   *
   * With `newKind` set, text that names no category or account can be
   * created (after a confirmation): "Fuel", or "Charity:Fast Offering"
   * for a subcategory. A new top-level category gets `newKind`; a new
   * subcategory takes its parent's kind.
   */
  let {
    value = $bindable(),
    excludeAccount,
    allowSplit = false,
    onchange,
    label = "Category",
    newKind,
  }: {
    value: TargetValue;
    excludeAccount?: AccountId;
    allowSplit?: boolean;
    onchange?: () => void;
    label?: string;
    newKind?: "income" | "expense";
  } = $props();

  const options = $derived.by((): TargetOption[] => {
    const out: TargetOption[] = [];
    if (allowSplit) out.push({ value: SPLIT, label: "--Split--" });
    for (const kind of ["income", "expense"]) {
      for (const c of listsState.categories) {
        if (c.kind !== kind) continue;
        if (c.hidden && value !== `c:${c.id}`) continue;
        out.push({ value: `c:${c.id}`, label: listsState.categoryPath(c.id) });
      }
    }
    for (const a of listsState.accounts) {
      if (a.id === excludeAccount || a.investment !== null) continue;
      if (a.status !== "open" && value !== `a:${a.id}`) continue;
      out.push({ value: `a:${a.id}`, label: `[${a.name}]` });
    }
    return out;
  });

  const labelOf = (v: TargetValue) =>
    options.find((o) => o.value === v)?.label ?? "";

  let text = $state("");
  let open = $state(false);
  let active = $state(0);
  let typing = $state(false);
  let input: HTMLInputElement;
  let pos = $state({ left: 0, top: 0, width: 0, up: false });
  const listId = `tc-${Math.random().toString(36).slice(2)}`;

  const matches = $derived(matchTargets(options, typing ? text : ""));
  /** The path a "Create" row would make, while typing text that is new. */
  const toCreate = $derived(newKind && typing ? newCategoryPath(options, text) : null);
  const rowCount = $derived(matches.length + (toCreate ? 1 : 0));
  const onCreateRow = $derived(toCreate !== null && active === matches.length);
  let creating = $state(false);
  let createError = $state<string | null>(null);

  // Show the chosen label unless the user is typing.
  $effect(() => {
    if (!typing) text = labelOf(value);
  });

  function place() {
    const r = input.getBoundingClientRect();
    const up = window.innerHeight - r.bottom < 240 && r.top > 240;
    pos = { left: r.left, top: up ? r.top : r.bottom, width: Math.max(r.width, 220), up };
  }

  function show() {
    place();
    open = true;
  }

  function choose(o: TargetOption | undefined) {
    typing = false;
    open = false;
    if (!o) {
      text = labelOf(value);
      return;
    }
    text = o.label;
    if (o.value !== value) {
      value = o.value;
      void tick().then(() => onchange?.());
    }
  }

  function oninput() {
    typing = true;
    active = 0;
    createError = null;
    if (text.trim() === "") {
      // An emptied box clears the choice.
      if (value !== "") {
        value = "";
        void tick().then(() => onchange?.());
      }
    }
    show();
  }

  /** Confirm, create the category (and any missing parents), and pick it. */
  async function createCategory() {
    const path = toCreate;
    if (path === null || !newKind || creating) return;
    creating = true;
    open = false;
    createError = null;
    try {
      const ok = await confirmState.ask(`Create new ${newKind} category "${path}"?`);
      if (ok) {
        const made = await call(commands.categoryCreatePath(path, newKind));
        await listsState.loadCategories();
        typing = false;
        value = `c:${made.id}`;
        text = labelOf(value);
        void tick().then(() => onchange?.());
      } else {
        typing = false;
        text = labelOf(value);
      }
    } catch (e) {
      createError = e instanceof Error ? e.message : String(e);
      open = true;
    } finally {
      creating = false;
      input?.focus();
    }
  }

  /** Enter or Tab on the highlighted row. */
  function commit() {
    if (onCreateRow) void createCategory();
    else choose(matches[active]);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!open) {
        show();
        return;
      }
      const n = rowCount;
      if (n > 0) active = (active + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
      document.getElementById(`${listId}-${active}`)?.scrollIntoView?.({ block: "nearest" });
    } else if (e.key === "Enter" && open) {
      e.preventDefault();
      e.stopPropagation();
      commit();
    } else if (e.key === "Escape" && open) {
      e.preventDefault();
      e.stopPropagation();
      choose(undefined);
    } else if (e.key === "Tab" && open && typing) {
      // Creating asks first, so keep the focus here until it is answered.
      if (onCreateRow) e.preventDefault();
      commit();
    }
  }

  function onblur() {
    if (creating) return;
    if (open && typing && !onCreateRow) choose(matches[active]);
    else choose(undefined);
  }
</script>

<input
  bind:this={input}
  class="combo"
  role="combobox"
  aria-label={label}
  aria-expanded={open}
  aria-controls={listId}
  aria-autocomplete="list"
  autocomplete="off"
  spellcheck="false"
  bind:value={text}
  {oninput}
  {onkeydown}
  {onblur}
  onfocus={(e) => e.currentTarget.select()}
/>

{#if open}
  <ul
    id={listId}
    class="list"
    role="listbox"
    style="left: {pos.left}px; width: {pos.width}px; {pos.up ? `bottom: ${window.innerHeight - pos.top}px` : `top: ${pos.top}px`}"
  >
    {#each matches as m, i (m.value)}
      <!-- mousedown, not click: click would arrive after the input blurred. -->
      <li
        id="{listId}-{i}"
        role="option"
        aria-selected={i === active}
        class:active={i === active}
        onmousedown={(e) => {
          e.preventDefault();
          choose(m);
        }}
      >
        {m.label}
      </li>
    {/each}
    {#if toCreate}
      <li
        id="{listId}-{matches.length}"
        role="option"
        aria-selected={onCreateRow}
        class="create"
        class:active={onCreateRow}
        onmousedown={(e) => {
          e.preventDefault();
          void createCategory();
        }}
      >
        + Create new {newKind} category “{toCreate}”
      </li>
    {:else if matches.length === 0}
      <li class="none" role="option" aria-selected="false" aria-disabled="true">No match</li>
    {/if}
    {#if createError}
      <li class="none" role="alert">Could not create it: {createError}</li>
    {/if}
  </ul>
{/if}

<style>
  .combo {
    width: 100%;
    min-width: 0;
    box-sizing: border-box;
    font: inherit;
  }
  .list {
    position: fixed;
    z-index: 70;
    margin: 0;
    padding: 0.15rem;
    list-style: none;
    max-height: 15rem;
    overflow-y: auto;
    background: var(--opt-bg, #fff);
    color: var(--opt-fg, #111);
    border: 1px solid #555;
    border-radius: 4px;
    box-shadow: 0 3px 10px rgba(0, 0, 0, 0.4);
  }
  li {
    padding: 0.2rem 0.5rem;
    cursor: pointer;
    white-space: nowrap;
  }
  li.active {
    background: var(--sel-bg, #1f6feb);
    color: var(--sel-fg, #fff);
  }
  li.create {
    border-top: 1px solid #555;
    font-weight: 600;
  }
  li.none {
    opacity: 0.6;
    cursor: default;
  }
</style>
