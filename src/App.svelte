<script lang="ts">
  import type { Component } from "svelte";
  import { themeState } from "./lib/state/theme.svelte";
  import { viewState } from "./lib/state/view.svelte";
  import { onMount } from "svelte";
  import AccountSelector from "./lib/components/AccountSelector.svelte";
  import { listsState } from "./lib/state/lists.svelte";
  import { settingsState } from "./lib/state/settings.svelte";
  import Account from "./views/Account.svelte";
  import AccountModal from "./lib/components/AccountModal.svelte";
  import ConfirmDialog from "./lib/components/ConfirmDialog.svelte";
  import HistoryModal from "./lib/components/HistoryModal.svelte";
  import IntegrityModal from "./lib/components/IntegrityModal.svelte";
  import { dialogState } from "./lib/state/dialogs.svelte";
  import Manage from "./views/Manage.svelte";
  import Dashboard from "./views/Dashboard.svelte";
  import EmptyBook from "./views/EmptyBook.svelte";

  onMount(() => {
    void listsState.loadAll();
  });

  // Placeholder until the other views exist (Phase 3+); Dashboard is the
  // only real one in Phase 0.
  const views: Record<string, Component> = {
    dashboard: Dashboard,
    account: Account,
    manage: Manage,
  };
  const View = $derived(views[viewState.current]);
</script>

<div
  class="app"
  data-theme={themeState.theme}
  style="font-size: {themeState.fontSize}px"
>
  <nav>
    <button onclick={() => viewState.navigate("dashboard")}>Dashboard</button>
    {#if settingsState.accountNav === "dropdown" && !listsState.isEmptyBook}
      <AccountSelector />
    {/if}
    <button onclick={() => viewState.navigate("manage")}>Payees, categories, tags</button>
    <button onclick={() => dialogState.newAccount()}>New account</button>
    <button onclick={() => (dialogState.integrity = true)}>Integrity check</button>
    <button onclick={() => settingsState.toggleAccountNav()}>
      Accounts: {settingsState.accountNav}
    </button>
    <button onclick={() => themeState.toggle()}>
      Toggle theme ({themeState.theme})
    </button>
  </nav>
  {#if listsState.error}<p class="err">{listsState.error}</p>{/if}
  {#if dialogState.account !== undefined}
    {#key dialogState.account?.id ?? "new"}<AccountModal account={dialogState.account} />{/key}
  {/if}
  {#if dialogState.history}<HistoryModal txn={dialogState.history.txn} />{/if}
  {#if dialogState.integrity}<IntegrityModal />{/if}
  <ConfirmDialog />
  <div class="body">
    {#if settingsState.accountNav === "sidebar" && !listsState.isEmptyBook}
      <AccountSelector />
    {/if}
    <main>
      {#if listsState.isEmptyBook}
        <EmptyBook />
      {:else if View}
        <View />
      {/if}
    </main>
  </div>
</div>

<style>
  :global(body) {
    margin: 0;
  }
  .app {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    --bg: #fff;
  }
  .app[data-theme="dark"] {
    --bg: #1e1e1e;
    background: #1e1e1e;
    color: #eee;
  }
  .app[data-theme="light"] {
    background: #fff;
    color: #111;
  }
  nav {
    flex-wrap: wrap;
    padding: 0.5rem;
    display: flex;
    gap: 0.5rem;
    border-bottom: 1px solid rgba(128, 128, 128, 0.3);
  }
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  main {
    padding: 1rem;
    flex: 1;
  }
  .err {
    color: #c0392b;
    margin: 0.5rem;
  }
</style>
