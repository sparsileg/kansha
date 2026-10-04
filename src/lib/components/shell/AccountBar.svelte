<script lang="ts">
  import { listsState } from "../../state/lists.svelte";
  import { settingsState } from "../../state/settings.svelte";
  import { statusState } from "../../state/status.svelte";
  import { rollDown } from "../../shell/motion";
  import GearButton from "../GearButton.svelte";
  import AccountList from "./AccountList.svelte";
  import ArrangeAccountsModal from "./ArrangeAccountsModal.svelte";

  /**
   * The thin bar under the navigation bar, at the account panel's side.
   * One framed bar as wide as the panel, with three click targets (a
   * button cannot hold buttons):
   *  - the triangle at its left opens the panel and keeps it open, or
   *    closes it (pointing down while open);
   *  - "Accounts" and the rest of the bar drop the list down to pick an
   *    account while the panel is closed (open, they are only a label);
   *  - the gear at its right arranges the list's sections (ACCT-240).
   *
   * The same row is the status bar: messages from statusState show on the
   * side away from the button, and clear themselves. An empty book has
   * no button, only the bar.
   */
  let drop = $state(false);
  let arranging = $state(false);
  let root: HTMLElement;
  const open = $derived(settingsState.accountPanelOpen);

  function togglePanel() {
    drop = false;
    settingsState.setAccountPanelOpen(!open);
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
  <span class="anchor" class:open>
    <span class="frame">
      <button
        type="button"
        class="pin"
        aria-label={open ? "Close the account list" : "Keep the account list open"}
        aria-expanded={open}
        title={open ? "Close the account list" : "Keep the account list open"}
        onclick={togglePanel}
      >
        <svg viewBox="0 0 10 10" aria-hidden="true" class:down={open}><path d="M2.5 1.5l5 3.5-5 3.5z" /></svg>
      </button>
      {#if open}
        <span class="head">Accounts</span>
      {:else}
        <button
          type="button"
          class="head"
          aria-expanded={drop}
          aria-haspopup="menu"
          title="Choose an account"
          onclick={() => (drop = !drop)}
        >
          Accounts
        </button>
      {/if}
      <GearButton label="Arrange accounts" onclick={() => ((drop = false), (arranging = true))} />
    </span>
    {#if drop}
      <div class="drop" role="menu" aria-label="Accounts" transition:rollDown>
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

{#if arranging}<ArrangeAccountsModal onclose={() => (arranging = false)} />{/if}

<style>
  /* No line under it: with the view's top padding (App.svelte) it makes
     one band under the navigation bar, and the button and the message sit
     midway down it (0.6rem above, 0.1rem plus the view's 0.5rem below). */
  .bar {
    display: flex;
    align-items: stretch;
    gap: 2rem;
    min-height: 1.6rem;
    padding: 0 0.5rem 0 0;
    font-size: var(--fs-ui);
  }
  .bar.right {
    flex-direction: row-reverse;
    padding: 0 0 0 0.5rem;
  }
  /* As wide as the panel, holding the framed bar. While the panel is open
     it takes the panel's color and edge; they slide out from the outer
     edge with the panel (motion.ts). */
  .anchor {
    position: relative;
    box-sizing: border-box;
    width: var(--account-panel-w);
    flex: none;
    display: flex;
    align-items: center;
    padding: 0.6rem 0.5rem 0.1rem;
  }
  .anchor::before {
    content: "";
    position: absolute;
    inset: 0;
    background: var(--panel-bg);
    border-right: 1px solid var(--line-soft);
    transform: scaleX(0);
    transform-origin: left;
    /* SLIDE_MS and cubicInOut, as the panel. */
    transition: transform 300ms cubic-bezier(0.65, 0, 0.35, 1);
  }
  .anchor.open::before {
    transform: scaleX(1);
  }
  .right .anchor::before {
    border-right: 0;
    border-left: 1px solid var(--line-soft);
    transform-origin: right;
  }
  @media (prefers-reduced-motion: reduce) {
    .anchor::before {
      transition: none;
    }
  }
  /* The message is pushed to the far side from the button: right when the
     button is on the left, left (against the window edge) when it is on
     the right. The gap keeps it clear of the button. */
  .status {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    padding-top: 0.5rem;
    margin-left: auto;
    min-width: 0;
    text-align: right;
  }
  .right .status {
    margin-left: 0;
    margin-right: auto;
    justify-content: flex-start;
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
  /* Looks like one button; the three targets inside share its edge. */
  .frame {
    position: relative; /* over the shading */
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    background: var(--btn-bg);
    color: var(--btn-fg);
    border: 1px solid var(--btn-border);
    border-radius: var(--btn-radius);
  }
  .frame > button {
    align-self: stretch;
    background: none;
    border: 0;
    color: inherit;
  }
  .frame > button:hover {
    background: var(--btn-hover-bg);
  }
  .frame > .pin {
    display: inline-flex;
    align-items: center;
    padding: 0 0.4rem;
    border-right: 1px solid var(--btn-border);
  }
  .head {
    flex: 1;
    min-width: 0;
    padding: 0.15rem 0.6rem;
    text-align: left;
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
  /* Edge to edge with the framed bar (the anchor's padding). */
  .drop {
    position: absolute;
    top: 100%;
    left: 0.5rem;
    right: 0.5rem;
    z-index: 40;
    box-sizing: border-box;
    max-height: 70vh;
    overflow-y: auto;
    padding: 0.5rem;
    background: var(--popup-bg);
    border: 1px solid var(--popup-border);
    box-shadow: var(--shadow-popup);
  }
</style>
