<script lang="ts">
  import { settingsState } from "../../state/settings.svelte";
  import AccountList from "./AccountList.svelte";

  /**
   * The thin bar under the navigation bar, at the account panel's side.
   * One button, "Accounts", with a triangle: pointing down while the panel
   * is open (click to close it), pointing sideways while it is closed
   * (click to drop the list down and pick an account). The drop-down has a
   * "Keep this list open" button to bring the panel back.
   */
  let drop = $state(false);
  let root: HTMLElement;
  const open = $derived(settingsState.accountPanelOpen);

  function toggle() {
    if (open) settingsState.setAccountPanelOpen(false);
    else drop = !drop;
  }
  function keepOpen() {
    drop = false;
    settingsState.setAccountPanelOpen(true);
  }
  function onOutside(e: PointerEvent) {
    if (drop && !root.contains(e.target as Node)) drop = false;
  }
  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && drop) {
      e.stopPropagation();
      drop = false;
    }
  }
</script>

<svelte:window onpointerdown={onOutside} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="bar" class:right={settingsState.accountPanelSide === "right"} bind:this={root} {onkeydown} role="group" aria-label="Account list">
  <span class="anchor">
    <button
      type="button"
      class="head"
      aria-expanded={open || drop}
      aria-haspopup={open ? undefined : "menu"}
      title={open ? "Close the account list" : "Choose an account"}
      onclick={toggle}
    >
      <svg viewBox="0 0 10 10" aria-hidden="true" class:down={open || drop}><path d="M2.5 1.5l5 3.5-5 3.5z" /></svg>
      Accounts
    </button>
    {#if drop && !open}
      <div class="drop" role="menu" aria-label="Accounts">
        <button type="button" class="keep" onclick={keepOpen}>Keep open</button>
        <AccountList onpick={() => (drop = false)} />
      </div>
    {/if}
  </span>
</div>

<style>
  .bar {
    display: flex;
    align-items: center;
    padding: 0.1rem 0.5rem;
    border-bottom: 1px solid rgba(128, 128, 128, 0.3);
    font-size: 0.9em;
  }
  .bar.right {
    flex-direction: row-reverse;
  }
  .anchor {
    position: relative;
  }
  .head {
    display: inline-flex;
    gap: 0.35rem;
    align-items: center;
    font-weight: 600;
  }
  svg {
    width: 0.7rem;
    height: 0.7rem;
    fill: currentColor;
  }
  svg.down {
    transform: rotate(90deg);
  }
  .drop {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 40;
    width: 16rem;
    max-height: 70vh;
    overflow-y: auto;
    padding: 0.5rem;
    background: var(--bg, #fff);
    border: 1px solid rgba(128, 128, 128, 0.6);
    box-shadow: 0 3px 10px rgba(0, 0, 0, 0.35);
  }
  .right .drop {
    left: auto;
    right: 0;
  }
  .keep {
    width: 100%;
    margin-bottom: 0.4rem;
    text-align: left;
  }
</style>
