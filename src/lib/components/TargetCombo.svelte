<script lang="ts">
  import { tick } from "svelte";
  import { SPLIT, type TargetValue } from "../register/draft";
  import { matchTargets, type TargetOption } from "../register/match";
  import { listsState } from "../state/lists.svelte";
  import type { AccountId } from "../types/bindings";

  /**
   * Category / transfer / split picker as a type-ahead box (REG-030).
   * Typing filters categories and accounts together; Up/Down move; Tab or
   * Enter (while the list is open) take the highlighted one; Esc closes
   * the list. With the list closed, Enter belongs to the form (save).
   */
  let {
    value = $bindable(),
    excludeAccount,
    allowSplit = false,
    onchange,
    label = "Category",
  }: {
    value: TargetValue;
    excludeAccount?: AccountId;
    allowSplit?: boolean;
    onchange?: () => void;
    label?: string;
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
    if (text.trim() === "") {
      // An emptied box clears the choice.
      if (value !== "") {
        value = "";
        void tick().then(() => onchange?.());
      }
    }
    show();
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!open) {
        show();
        return;
      }
      const n = matches.length;
      if (n > 0) active = (active + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
      document.getElementById(`${listId}-${active}`)?.scrollIntoView?.({ block: "nearest" });
    } else if (e.key === "Enter" && open) {
      e.preventDefault();
      e.stopPropagation();
      choose(matches[active]);
    } else if (e.key === "Escape" && open) {
      e.preventDefault();
      e.stopPropagation();
      choose(undefined);
    } else if (e.key === "Tab" && open && typing) {
      choose(matches[active]);
    }
  }

  function onblur() {
    if (open && typing) choose(matches[active]);
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
    {:else}
      <li class="none" role="option" aria-selected="false" aria-disabled="true">No match</li>
    {/each}
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
    background: #fff;
    color: #111;
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
    background: #1f6feb;
    color: #fff;
  }
  li.none {
    opacity: 0.6;
    cursor: default;
  }
</style>
