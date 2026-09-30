<script lang="ts">
  import { formatMoney } from "../../format/money";
  import { listsState } from "../../state/lists.svelte";
  import { settingsState } from "../../state/settings.svelte";
  import AccountList from "./AccountList.svelte";
</script>

<aside class="panel" class:right={settingsState.accountPanelSide === "right"} aria-label="Accounts">
  <div class="list"><AccountList /></div>
  <!-- Net worth today, from Rust (ACCT-240). -->
  <footer>
    <span>Net Worth</span>
    <span class="num" class:neg={listsState.netWorth?.startsWith("-")}>
      {listsState.netWorth === null ? "—" : formatMoney(listsState.netWorth)}
    </span>
  </footer>
</aside>

<style>
  /* The header above it (the Accounts bar's toggle) takes the same width. */
  .panel {
    box-sizing: border-box;
    width: var(--account-panel-w);
    flex: none;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--line-soft);
    background: var(--panel-bg);
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
