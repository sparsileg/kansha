<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    onclose,
    wide = false,
    children,
  }: { title: string; onclose: () => void; wide?: boolean; children: Snippet } =
    $props();

  let dialog: HTMLDivElement;

  $effect(() => {
    // Focus the first control so the dialog is keyboard-usable at once.
    dialog
      .querySelector<HTMLElement>("input, select, textarea, button")
      ?.focus();
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      onclose();
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" {onkeydown}>
  <div
    class="modal"
    class:wide
    role="dialog"
    aria-modal="true"
    aria-label={title}
    bind:this={dialog}
  >
    <header>
      <h2>{title}</h2>
      <button type="button" aria-label="Close" onclick={onclose}>×</button>
    </header>
    {@render children()}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 4vh;
    z-index: 50;
  }
  .modal {
    background: var(--bg, #fff);
    color: inherit;
    border: 1px solid rgba(128, 128, 128, 0.5);
    border-radius: 6px;
    padding: 1rem;
    width: min(34rem, 94vw);
    max-height: 90vh;
    overflow-y: auto;
  }
  .modal.wide {
    width: min(56rem, 96vw);
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h2 {
    margin: 0 0 0.5rem;
    font-size: 1.15em;
  }
</style>
