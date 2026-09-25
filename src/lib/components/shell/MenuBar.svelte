<script lang="ts">
  import { tick } from "svelte";
  import type { Menu } from "../../shell/menus";

  /**
   * The menu bar. Click a menu to open it; with one open, moving over
   * another switches to it. Keys: Down or Enter opens, Up/Down move among
   * the enabled items, Left/Right switch menus, Esc closes, Tab leaves.
   * Greyed items stay visible and say why they are unavailable.
   */
  let { menus, onselect }: { menus: Menu[]; onselect: (id: string) => void } = $props();

  let open = $state<number | null>(null);
  let root: HTMLElement;
  const buttons = $state<HTMLButtonElement[]>([]);

  const enabledItems = (menu: number) =>
    Array.from(
      root.querySelectorAll<HTMLElement>(`[data-menu="${menu}"] [role="menuitem"]:not([aria-disabled="true"])`),
    );

  async function openMenu(i: number, focusItem = false) {
    open = i;
    if (!focusItem) return;
    await tick();
    enabledItems(i)[0]?.focus();
  }

  function close(refocus = false) {
    const was = open;
    open = null;
    if (refocus && was !== null) buttons[was]?.focus();
  }

  function pick(item: { id: string; disabled?: string }) {
    if (item.disabled) return;
    close();
    onselect(item.id);
  }

  function onButtonKey(e: KeyboardEvent, i: number) {
    const n = menus.length;
    if (e.key === "ArrowDown" || e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      void openMenu(i, true);
    } else if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      const next = (i + (e.key === "ArrowRight" ? 1 : n - 1)) % n;
      buttons[next]?.focus();
      if (open !== null) void openMenu(next);
    } else if (e.key === "Escape") {
      close();
    }
  }

  function onMenuKey(e: KeyboardEvent, i: number) {
    const items = enabledItems(i);
    const at = items.indexOf(document.activeElement as HTMLElement);
    const n = menus.length;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (items.length === 0) return;
      const step = e.key === "ArrowDown" ? 1 : -1;
      items[(at + step + items.length) % items.length].focus();
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      items[e.key === "Home" ? 0 : items.length - 1]?.focus();
    } else if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      const next = (i + (e.key === "ArrowRight" ? 1 : n - 1)) % n;
      buttons[next]?.focus();
      void openMenu(next, true);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close(true);
    } else if (e.key === "Tab") {
      close();
    }
  }

  function onOutside(e: PointerEvent) {
    if (open !== null && !root.contains(e.target as Node)) close();
  }
</script>

<svelte:window onpointerdown={onOutside} />

<nav class="menubar" aria-label="Menu bar" bind:this={root}>
  {#each menus as menu, i (menu.id)}
    <div class="menu">
      <button
        type="button"
        bind:this={buttons[i]}
        aria-haspopup="menu"
        aria-expanded={open === i}
        class:on={open === i}
        onclick={() => (open === i ? close() : void openMenu(i))}
        onmouseenter={() => open !== null && open !== i && void openMenu(i)}
        onkeydown={(e) => onButtonKey(e, i)}
      >
        {menu.label}
      </button>
      {#if open === i}
        <!-- svelte-ignore a11y_interactive_supports_focus -->
        <div class="list" role="menu" aria-label={menu.label} data-menu={i} onkeydown={(e) => onMenuKey(e, i)}>
          {#each menu.items as item (item.id)}
            {#if item.divider}<hr />{/if}
            <button
              type="button"
              role="menuitem"
              class:off={!!item.disabled}
              aria-disabled={item.disabled ? "true" : undefined}
              title={item.disabled ?? ""}
              onclick={() => pick(item)}
            >
              <span>{item.label}</span>
              {#if item.disabled}<span class="hint">{item.disabled}</span>{/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/each}
</nav>

<style>
  .menubar {
    display: flex;
    padding: 0 0.25rem;
    border-bottom: 1px solid rgba(128, 128, 128, 0.3);
  }
  .menu {
    position: relative;
  }
  .menu > button {
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    padding: 0.3rem 0.7rem;
    cursor: pointer;
  }
  .menu > button:hover,
  .menu > button.on {
    background: rgba(128, 128, 128, 0.25);
  }
  .list {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 40;
    min-width: 14rem;
    padding: 0.2rem 0;
    background: var(--bg, #fff);
    color: inherit;
    border: 1px solid rgba(128, 128, 128, 0.6);
    box-shadow: 0 3px 10px rgba(0, 0, 0, 0.35);
  }
  .list button {
    display: flex;
    justify-content: space-between;
    gap: 1.5rem;
    width: 100%;
    text-align: left;
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    padding: 0.3rem 0.9rem;
    cursor: pointer;
  }
  .list button:hover:not(.off),
  .list button:focus-visible {
    background: var(--sel-bg, #1f6feb);
    color: var(--sel-fg, #fff);
    outline: none;
  }
  .list button.off {
    cursor: default;
    opacity: 0.6;
  }
  .hint {
    font-size: 0.8em;
    align-self: center;
  }
  hr {
    border: 0;
    border-top: 1px solid rgba(128, 128, 128, 0.4);
    margin: 0.2rem 0;
  }
</style>
