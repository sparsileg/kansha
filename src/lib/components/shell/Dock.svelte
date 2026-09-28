<script lang="ts">
  // The dock: one label per open window (any kind). Clicking a label
  // brings that window to the top; clicking the one on top puts it back in
  // the dock. The window on top is marked by a heavy border and bold text,
  // not by color alone.
  import { windowState } from "../../state/windows.svelte";

  const labels = $derived(windowState.labels());
</script>

{#if windowState.wins.length}
  <nav class="dock no-print" aria-label="Open windows">
    {#each windowState.wins as w (w.id)}
      {@const label = labels.get(w.id) ?? ""}
      {@const top = windowState.shown === w.id}
      <span class="item" class:top>
        <button
          type="button"
          class="show"
          aria-current={top ? "page" : undefined}
          title={top ? `Minimize ${label}` : `Show ${label}`}
          onclick={() => (top ? windowState.minimize() : windowState.show(w.id))}>{label}</button
        >
        <button type="button" class="x" aria-label="Close {label}" title="Close" onclick={() => void windowState.close(w.id)}
          >×</button
        >
      </span>
    {/each}
  </nav>
{/if}

<style>
  .dock {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    padding: 0.25rem 0.5rem;
    border-top: 1px solid rgba(128, 128, 128, 0.5);
  }
  .item {
    display: inline-flex;
    align-items: stretch;
    border: 1px solid rgba(128, 128, 128, 0.6);
    border-radius: 3px;
  }
  .item.top {
    border: 2px solid currentColor;
    font-weight: 700;
  }
  /* Button text at the browser's button size, like the navigation bar. */
  .item button {
    background: none;
    border: 0;
    color: inherit;
    font-weight: inherit;
    cursor: pointer;
    padding: 0.15rem 0.5rem;
  }
  .item .x {
    padding: 0.15rem 0.4rem;
    border-left: 1px solid rgba(128, 128, 128, 0.5);
  }
  .item button:hover {
    background: rgba(128, 128, 128, 0.25);
  }
</style>
