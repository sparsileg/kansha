<script lang="ts">
  import { tick, type Snippet } from "svelte";
  import type { Menu } from "../../shell/menus";

  /**
   * The menu bar. Click a menu to open it; with one open, moving over
   * another switches to it. Keys: Down or Enter opens, Up/Down move among
   * the enabled items, Left/Right switch menus, Esc closes, Tab leaves.
   * An item marked ▸ opens a submenu (hover, click, Enter, or Right);
   * Left or Esc goes back to it. A submenu item marked ▸ opens a third
   * level the same way (Reports > Saved Reports > a folder). Greyed items stay visible and say why
   * they are unavailable.
   */
  let {
    menus,
    onselect,
    children,
  }: {
    menus: Menu[];
    onselect: (id: string) => void;
    /** Controls at the right end of the bar (they take their own Tab stops). */
    children?: Snippet;
  } = $props();

  let open = $state<number | null>(null);
  /** The open submenu: its item's index in the open menu. */
  let sub = $state<number | null>(null);
  /** The open third-level menu: its item's index in the submenu. */
  let sub2 = $state<number | null>(null);
  let root: HTMLElement;
  const buttons = $state<HTMLButtonElement[]>([]);

  const enabledItems = (menu: number) =>
    Array.from(
      root.querySelectorAll<HTMLElement>(
        `[data-menu="${menu}"] [role="menuitem"][data-top]:not([aria-disabled="true"])`,
      ),
    );
  const subItems = () =>
    Array.from(root.querySelectorAll<HTMLElement>(`[data-sub] [role="menuitem"][data-mid]:not([aria-disabled="true"])`));
  const sub2Items = () =>
    Array.from(root.querySelectorAll<HTMLElement>(`[data-sub2] [role="menuitem"]:not([aria-disabled="true"])`));

  async function openMenu(i: number, focusItem = false) {
    open = i;
    sub = null;
    sub2 = null;
    if (!focusItem) return;
    await tick();
    enabledItems(i)[0]?.focus();
  }

  async function openSub(j: number, focusItem = false) {
    if (sub !== j) sub2 = null;
    sub = j;
    if (!focusItem) return;
    await tick();
    subItems()[0]?.focus();
  }

  async function openSub2(k: number, focusItem = false) {
    sub2 = k;
    if (!focusItem) return;
    await tick();
    sub2Items()[0]?.focus();
  }

  /** Close the third-level menu and go back to its item. */
  function closeSub2() {
    const k = sub2;
    sub2 = null;
    if (k !== null) root.querySelector<HTMLElement>(`[data-sub] [data-index2="${k}"]`)?.focus();
  }

  /** Close the submenu and go back to its item. */
  function closeSub() {
    const j = sub;
    sub = null;
    sub2 = null;
    if (open !== null && j !== null) {
      root.querySelector<HTMLElement>(`[data-menu="${open}"] [data-index="${j}"]`)?.focus();
    }
  }

  function close(refocus = false) {
    const was = open;
    open = null;
    sub = null;
    sub2 = null;
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
    const here = document.activeElement as HTMLElement | null;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (items.length === 0) return;
      sub = null;
      sub2 = null;
      const step = e.key === "ArrowDown" ? 1 : -1;
      items[(at + step + items.length) % items.length].focus();
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      sub = null;
      items[e.key === "Home" ? 0 : items.length - 1]?.focus();
    } else if (e.key === "ArrowRight" && here?.dataset.index !== undefined && here.getAttribute("aria-haspopup")) {
      e.preventDefault();
      void openSub(Number(here.dataset.index), true);
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

  function onSubKey(e: KeyboardEvent, i: number) {
    const items = subItems();
    const at = items.indexOf(document.activeElement as HTMLElement);
    const here = document.activeElement as HTMLElement | null;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      e.stopPropagation();
      if (items.length === 0) return;
      sub2 = null;
      const step = e.key === "ArrowDown" ? 1 : -1;
      items[(at + step + items.length) % items.length].focus();
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      e.stopPropagation();
      items[e.key === "Home" ? 0 : items.length - 1]?.focus();
    } else if (e.key === "ArrowLeft" || e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      closeSub();
    } else if (e.key === "ArrowRight" && here?.dataset.index2 !== undefined) {
      e.preventDefault();
      e.stopPropagation();
      void openSub2(Number(here.dataset.index2), true);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      e.stopPropagation();
      const next = (i + 1) % menus.length;
      buttons[next]?.focus();
      void openMenu(next, true);
    }
  }

  function onSub2Key(e: KeyboardEvent, i: number) {
    const items = sub2Items();
    const at = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      e.stopPropagation();
      if (items.length === 0) return;
      const step = e.key === "ArrowDown" ? 1 : -1;
      items[(at + step + items.length) % items.length].focus();
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      e.stopPropagation();
      items[e.key === "Home" ? 0 : items.length - 1]?.focus();
    } else if (e.key === "ArrowLeft" || e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      closeSub2();
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      e.stopPropagation();
      const next = (i + 1) % menus.length;
      buttons[next]?.focus();
      void openMenu(next, true);
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
          {#each menu.items as item, j (item.id)}
            {#if item.divider}<hr />{/if}
            {#if item.items}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="subwrap" onmouseenter={() => void openSub(j)}>
                <button
                  type="button"
                  role="menuitem"
                  data-top
                  data-index={j}
                  aria-haspopup="menu"
                  aria-expanded={sub === j}
                  class:on={sub === j}
                  onclick={() => void openSub(j, true)}
                >
                  <span>{item.label}</span>
                  <span class="arrow" aria-hidden="true">▸</span>
                </button>
                {#if sub === j}
                  <!-- svelte-ignore a11y_interactive_supports_focus -->
                  <div class="list sub" role="menu" aria-label={item.label} data-sub onkeydown={(e) => onSubKey(e, i)}>
                    {#each item.items as s, k (s.id)}
                      {#if s.divider}<hr />{/if}
                      {#if s.items}
                        <!-- svelte-ignore a11y_no_static_element_interactions -->
                        <div class="subwrap" onmouseenter={() => void openSub2(k)}>
                          <button
                            type="button"
                            role="menuitem"
                            data-mid
                            data-index2={k}
                            aria-haspopup="menu"
                            aria-expanded={sub2 === k}
                            class:on={sub2 === k}
                            onclick={() => void openSub2(k, true)}
                          >
                            <span>{s.label}</span>
                            <span class="arrow" aria-hidden="true">▸</span>
                          </button>
                          {#if sub2 === k}
                            <!-- svelte-ignore a11y_interactive_supports_focus -->
                            <div class="list sub" role="menu" aria-label={s.label} data-sub2 onkeydown={(e) => onSub2Key(e, i)}>
                              {#each s.items as t (t.id)}
                                {#if t.divider}<hr />{/if}
                                <button
                                  type="button"
                                  role="menuitem"
                                  class:off={!!t.disabled}
                                  aria-disabled={t.disabled ? "true" : undefined}
                                  title={t.disabled ?? ""}
                                  onclick={() => pick(t)}
                                >
                                  <span>{t.label}</span>
                                  {#if t.disabled}<span class="hint">{t.disabled}</span>{/if}
                                </button>
                              {/each}
                            </div>
                          {/if}
                        </div>
                      {:else}
                        <button
                          type="button"
                          role="menuitem"
                          data-mid
                          class:off={!!s.disabled}
                          aria-disabled={s.disabled ? "true" : undefined}
                          title={s.disabled ?? ""}
                          onmouseenter={() => (sub2 = null)}
                          onclick={() => pick(s)}
                        >
                          <span>{s.label}</span>
                          {#if s.disabled}<span class="hint">{s.disabled}</span>{/if}
                        </button>
                      {/if}
                    {/each}
                  </div>
                {/if}
              </div>
            {:else}
              <button
                type="button"
                role="menuitem"
                data-top
                class:off={!!item.disabled}
                aria-disabled={item.disabled ? "true" : undefined}
                title={item.disabled ?? ""}
                onmouseenter={() => (sub = null)}
                onclick={() => pick(item)}
              >
                <span>{item.label}</span>
                {#if item.disabled}<span class="hint">{item.disabled}</span>{/if}
              </button>
            {/if}
          {/each}
        </div>
      {/if}
    </div>
  {/each}
  {@render children?.()}
</nav>

<style>
  .menubar {
    display: flex;
    align-items: center;
    background: var(--menubar-bg);
    color: var(--menubar-fg);
    padding: 0 0.25rem;
    border-bottom: 1px solid var(--line-soft);
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
    background: var(--hover-bg);
  }
  .list {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 40;
    /* As wide as the longest item: titles never wrap. */
    width: max-content;
    min-width: 14rem;
    padding: 0.2rem 0;
    background: var(--popup-bg);
    color: inherit;
    border: 1px solid var(--popup-border);
    box-shadow: var(--shadow-popup);
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
    white-space: nowrap;
  }
  .list button:hover:not(.off),
  .list button:focus-visible {
    background: var(--sel-bg);
    color: var(--sel-fg);
    outline: none;
  }
  .list button.on {
    background: var(--hover-bg);
  }
  .subwrap {
    position: relative;
  }
  .list.sub {
    top: -0.2rem;
    left: 100%;
  }
  .arrow {
    align-self: center;
  }
  .list button.off {
    cursor: default;
    opacity: 0.6;
  }
  .hint {
    font-size: var(--fs-small);
    align-self: center;
  }
  hr {
    border: 0;
    border-top: 1px solid var(--line-soft);
    margin: 0.2rem 0;
  }
</style>
