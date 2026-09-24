<script lang="ts">
  import { settingsState } from "../../state/settings.svelte";
  import AccountList from "./AccountList.svelte";
</script>

<aside class="panel" class:right={settingsState.accountPanelSide === "right"} aria-label="Accounts">
  <AccountList />
</aside>

<style>
  .panel {
    width: 15rem;
    flex: none;
    padding: 0.5rem;
    border-right: 1px solid rgba(128, 128, 128, 0.3);
    overflow-y: auto;
  }
  .panel.right {
    border-right: 0;
    border-left: 1px solid rgba(128, 128, 128, 0.3);
  }
</style>
