<script lang="ts">
  // A window's frame: its name (the window's heading), Minimize (back to
  // the dock), and Close. The window fills the view area; what it shows
  // comes from `children`, which has no heading of its own (a report's
  // page title is part of the printed page).
  import type { Snippet } from "svelte";
  import { windowState } from "../../state/windows.svelte";

  let { id, children }: { id: number; children: Snippet } = $props();

  const label = $derived(windowState.labels().get(id) ?? "");
</script>

<section class="window" aria-label={label}>
  <div class="frame no-print">
    <h1 class="name">{label}</h1>
    <button type="button" onclick={() => windowState.minimize()} title="Keep it open in the dock">
      <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"><path d="M3 12.5h10" /></svg>
      Minimize
    </button>
    <button type="button" onclick={() => void windowState.close(id)}>
      <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
      Close
    </button>
  </div>
  {@render children()}
</section>

<style>
  .window {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
  .frame {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding-bottom: 0.4rem;
    margin-bottom: 0.4rem;
    border-bottom: 1px solid rgba(128, 128, 128, 0.5);
  }
  .name {
    flex: 1;
    margin: 0;
    font-size: 1.3em;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .frame button {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
  }
  svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
  }
</style>
