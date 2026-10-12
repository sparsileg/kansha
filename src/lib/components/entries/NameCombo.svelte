<script lang="ts">
  import { tick } from "svelte";
  import { completeSegment, matchTargets, type TargetOption } from "../../register/match";

  /**
   * A name box for the review list's Edit row (CLI-040): type-ahead over
   * the book's account or category names, as the register's Category
   * box (REG-030). Up/Down move; Tab or Enter (while the list is open)
   * take the highlighted name; Esc closes the list. With `paths`, a
   * category path level fills in ("Car:B" → "Car:BlueForester"). The
   * value is text: a name not listed stays as typed, and review checks
   * it against the book.
   */
  let {
    value = $bindable(),
    names,
    label,
    paths = false,
  }: {
    value: string;
    names: string[];
    label: string;
    /** Category paths: fill in the path level being typed. */
    paths?: boolean;
  } = $props();

  // matchTargets and completeSegment work on options; "c:" marks the
  // names completeSegment fills in.
  const options = $derived<TargetOption[]>(names.map((n) => ({ value: `${paths ? "c" : "a"}:${n}`, label: n })));

  /** What the user typed, without a filled-in completion; the list filters on it. */
  let query = $state("");
  let open = $state(false);
  let active = $state(0);
  let input: HTMLInputElement;
  let pos = $state({ left: 0, top: 0, width: 0, up: false });
  const listId = `nc-${Math.random().toString(36).slice(2)}`;

  const matches = $derived(matchTargets(options, query));

  function show() {
    const r = input.getBoundingClientRect();
    const up = window.innerHeight - r.bottom < 240 && r.top > 240;
    pos = { left: r.left, top: up ? r.top : r.bottom, width: Math.max(r.width, 220), up };
    open = true;
  }

  function choose(o: TargetOption | undefined) {
    open = false;
    if (o) value = o.label;
  }

  function oninput(e: Event) {
    active = 0;
    query = value;
    // Deleting removes a completion; it does not bring it back.
    const deleting = (e as InputEvent).inputType?.startsWith("delete") ?? false;
    const done = paths && !deleting ? completeSegment(options, value) : null;
    if (done) {
      value = done.text;
      const at = matches.findIndex((m) => m.value === done.value);
      if (at >= 0) active = at;
      const from = query.length;
      void tick().then(() => input?.setSelectionRange(from, done.text.length));
    }
    show();
  }

  function onkeydown(e: KeyboardEvent) {
    const filled = value !== query && input.selectionStart === query.length && input.selectionEnd === value.length;
    if (e.key === ":" && filled) {
      // ":" keeps the filled-in name and goes on to the next level.
      input.setSelectionRange(value.length, value.length);
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!open) {
        query = "";
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
      open = false;
    } else if (e.key === "Tab" && open) {
      choose(matches[active]);
    }
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
  bind:value
  {oninput}
  {onkeydown}
  onblur={() => (open = false)}
  onfocus={(e) => e.currentTarget.select()}
/>

{#if open && matches.length > 0}
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
  </ul>
{/if}

<style>
  .combo {
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
    background: var(--opt-bg);
    color: var(--opt-fg);
    border: 1px solid var(--popup-border);
    border-radius: 4px;
    box-shadow: var(--shadow-popup);
  }
  li {
    padding: 0.2rem 0.5rem;
    cursor: pointer;
    white-space: nowrap;
  }
  li.active {
    background: var(--sel-bg);
    color: var(--sel-fg);
  }
</style>
