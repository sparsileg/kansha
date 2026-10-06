<script lang="ts">
  import { formatMoney, formatMoneyWhole } from "../../format/money";
  import { bookSettings } from "../../state/booksettings.svelte";
  import { listsState } from "../../state/lists.svelte";
  import { settingsState } from "../../state/settings.svelte";
  import { drawer } from "../../shell/motion";
  import AccountList from "./AccountList.svelte";

  const MIN = 160;
  const STEP = 16;
  /** The widest the panel may be: most of the window never. */
  const max = () => Math.max(MIN, Math.min(800, Math.floor(window.innerWidth * 0.6)));
  const clamp = (w: number) => Math.round(Math.min(max(), Math.max(MIN, w)));

  let panel = $state<HTMLElement>();
  let drag: { x: number; width: number } | null = null;
  const right = $derived(settingsState.accountPanelSide === "right");

  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { x: e.clientX, width: panel!.getBoundingClientRect().width };
    e.preventDefault();
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    // The edge toward the register: dragging away from the panel widens it.
    const dx = e.clientX - drag.x;
    settingsState.liveWidth = clamp(drag.width + (right ? -dx : dx));
  }
  function up() {
    if (!drag) return;
    drag = null;
    const w = settingsState.liveWidth;
    if (w !== null) settingsState.setAccountPanelWidth(w);
    settingsState.liveWidth = null;
  }
  function key(e: KeyboardEvent) {
    const grow = (e.key === "ArrowRight") !== right;
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      const now = panel!.getBoundingClientRect().width;
      settingsState.setAccountPanelWidth(clamp(now + (grow ? STEP : -STEP)));
      e.preventDefault();
    } else if (e.key === "Home" || e.key === "Escape") {
      // The stock width.
      settingsState.setAccountPanelWidth(0);
    }
  }
</script>

<aside class="panel" class:right bind:this={panel} aria-label="Accounts" transition:drawer>
  <!-- Drag the edge to change the width; arrows nudge it, Home or a double
       click restores the stock width. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="grip"
    role="separator"
    aria-orientation="vertical"
    aria-label="Resize the account list"
    aria-valuenow={Math.round(panel?.getBoundingClientRect().width ?? 0)}
    aria-valuemin={MIN}
    aria-valuemax={max()}
    tabindex="0"
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
    onkeydown={key}
    ondblclick={() => settingsState.setAccountPanelWidth(0)}
  ></div>
  <div class="inner">
    <div class="list"><AccountList /></div>
    <!-- Net worth today, from Rust (ACCT-240). -->
    <footer>
      <span>Net Worth</span>
      <span class="num" class:neg={listsState.netWorth?.startsWith("-")}>
        {listsState.netWorth === null
          ? "—"
          : bookSettings.value.account_bar_cents
            ? formatMoney(listsState.netWorth)
            : formatMoneyWhole(listsState.netWorth)}
      </span>
    </footer>
  </div>
</aside>

<style>
  /* The header above it (the Accounts bar) takes the same width. It stops
     short of the window's bottom by the view's padding, level with the
     sheet beside it. */
  .panel {
    box-sizing: border-box;
    width: var(--account-panel-w);
    flex: none;
    display: flex;
    justify-content: flex-end;
    margin-bottom: 1rem;
    border-right: 1px solid var(--line-soft);
    border-bottom: 1px solid var(--line-soft);
    background: var(--panel-bg);
    position: relative;
  }
  /* Full width even while the panel slides (motion.ts): held against the
     inner edge, it comes out from under the outer edge like a drawer. */
  .inner {
    box-sizing: border-box;
    width: calc(var(--account-panel-w) - 1px);
    flex: none;
    display: flex;
    flex-direction: column;
  }
  .panel.right {
    justify-content: flex-start;
  }
  /* A strip over the panel's inner edge, the register's side. */
  .grip {
    position: absolute;
    top: 0;
    bottom: 0;
    right: -3px;
    width: 7px;
    cursor: col-resize;
    z-index: 2;
    touch-action: none;
  }
  .panel.right .grip {
    right: auto;
    left: -3px;
  }
  .grip:hover,
  .grip:focus-visible,
  .grip:active {
    background: var(--hover-bg);
  }
  .panel.right {
    border-right: 0;
    border-left: 1px solid var(--line-soft);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0.5rem;
  }
  footer {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.4rem 0.9rem;
    border-top: 1px solid var(--line);
    font-weight: 700;
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .neg {
    color: var(--bad);
  }
</style>
