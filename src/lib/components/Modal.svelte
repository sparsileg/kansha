<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    onclose,
    wide = false,
    fit = false,
    side = false,
    top = false,
    children,
  }: {
    title: string;
    onclose: () => void;
    wide?: boolean;
    /** As wide as its widest row (up to the window), never narrower than `wide`. */
    fit?: boolean;
    /** Docked to the right with the page still usable beside it: no dimming, clicks pass through. */
    side?: boolean;
    /** Above every other dialog, menu, and drop-down: the Confirm dialog,
     * which other dialogs open. */
    top?: boolean;
    children: Snippet;
  } = $props();

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
<div class="backdrop" class:side class:top {onkeydown}>
  <div
    class="modal"
    class:wide
    class:fit
    role="dialog"
    aria-modal={!side}
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
    background: var(--backdrop);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 4vh;
    z-index: 50;
  }
  .backdrop.top {
    z-index: 90;
  }
  .backdrop.side {
    background: none;
    pointer-events: none;
    justify-content: flex-end;
    padding-right: 1rem;
  }
  .backdrop.side .modal {
    pointer-events: auto;
    width: min(40rem, 94vw);
    box-shadow: var(--shadow-popup);
  }
  .modal {
    background: var(--popup-bg);
    color: inherit;
    border: 1px solid var(--popup-border);
    border-radius: 6px;
    padding: 1rem;
    width: min(34rem, 94vw);
    max-height: 90vh;
    overflow-y: auto;
  }
  .modal.wide {
    width: min(56rem, 96vw);
  }
  .modal.fit {
    width: max-content;
    min-width: min(56rem, 96vw);
    max-width: 96vw;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h2 {
    margin: 0 0 0.5rem;
    font-size: var(--fs-heading);
  }
</style>
