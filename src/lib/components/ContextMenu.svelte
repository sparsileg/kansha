<script lang="ts">
  export interface MenuItem {
    label: string;
    action: () => void;
    disabled?: boolean;
  }

  let {
    x,
    y,
    items,
    onclose,
  }: { x: number; y: number; items: MenuItem[]; onclose: () => void } = $props();

  let menu: HTMLDivElement;

  $effect(() => {
    menu.querySelector<HTMLElement>("button:not(:disabled)")?.focus();
  });

  function onkeydown(e: KeyboardEvent) {
    const btns = [...menu.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
    const i = btns.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      btns[(i + 1) % btns.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      btns[(i - 1 + btns.length) % btns.length]?.focus();
    }
  }
</script>

<svelte:window onclick={onclose} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="menu"
  role="menu"
  tabindex="-1"
  style="left: {x}px; top: {y}px"
  bind:this={menu}
  {onkeydown}
>
  {#each items as it (it.label)}
    <button
      type="button"
      role="menuitem"
      disabled={it.disabled}
      onclick={() => {
        onclose();
        it.action();
      }}>{it.label}</button
    >
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 60;
    background: var(--bg, #fff);
    color: inherit;
    border: 1px solid rgba(128, 128, 128, 0.6);
    border-radius: 4px;
    display: flex;
    flex-direction: column;
    min-width: 11rem;
    padding: 0.2rem;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  }
  button {
    text-align: left;
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    padding: 0.25rem 0.5rem;
    cursor: pointer;
  }
  button:hover:not(:disabled),
  button:focus {
    background: rgba(128, 128, 128, 0.25);
    outline: none;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
