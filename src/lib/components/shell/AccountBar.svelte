<script lang="ts">
  import { listsState } from "../../state/lists.svelte";
  import { settingsState } from "../../state/settings.svelte";
  import { statusState } from "../../state/status.svelte";
  import AccountList from "./AccountList.svelte";

  /**
   * The thin bar under the navigation bar, at the account panel's side.
   * One button, "Accounts", with a triangle: pointing down while the panel
   * is open (click to close it), pointing sideways while it is closed
   * (click to drop the list down and pick an account). The drop-down has a
   * "Keep this list open" button to bring the panel back.
   *
   * The same row is the status bar: messages from statusState show on the
   * side away from the button, and clear themselves. An empty book has
   * no button, only the bar.
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
  {#if !listsState.isEmptyBook}
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
  {/if}
  <!-- Always present, so a screen reader hears what is put in it. -->
  <span class="status" class:alert={statusState.message?.kind === "alert"} role="status">
    {statusState.message?.text ?? ""}
  </span>
</div>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 2rem;
    min-height: 1.6rem;
    padding: 0.1rem 0.5rem;
    border-bottom: 1px solid var(--line-soft);
    font-size: var(--fs-ui);
  }
  .bar.right {
    flex-direction: row-reverse;
  }
  .anchor {
    position: relative;
  }
  /* The message is pushed to the far side from the button: right when the
     button is on the left, left (against the window edge) when it is on
     the right. The gap keeps it clear of the button. */
  .status {
    margin-left: auto;
    min-width: 0;
    text-align: right;
  }
  .right .status {
    margin-left: 0;
    margin-right: auto;
    text-align: left;
  }
  /* An alert flashes once a second and is bold, so it does not rely on
     color. Those who ask for less motion get the bold text steady. */
  .status.alert {
    color: var(--bad);
    font-weight: 700;
    animation: flash 1s steps(1, end) infinite;
  }
  @keyframes flash {
    50% {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .status.alert {
      animation: none;
    }
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
    width: min(16rem, 90vw);
    max-height: 70vh;
    overflow-y: auto;
    padding: 0.5rem;
    background: var(--popup-bg);
    border: 1px solid var(--popup-border);
    box-shadow: var(--shadow-popup);
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
