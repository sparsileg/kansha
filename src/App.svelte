<script lang="ts">
  import type { Component } from "svelte";
  import { onMount } from "svelte";
  import Account from "./views/Account.svelte";
  import Accounts from "./views/Accounts.svelte";
  import AccountModal from "./lib/components/AccountModal.svelte";
  import ConfirmDialog from "./lib/components/ConfirmDialog.svelte";
  import DueDialog from "./lib/components/DueDialog.svelte";
  import HistoryModal from "./lib/components/HistoryModal.svelte";
  import IntegrityModal from "./lib/components/IntegrityModal.svelte";
  import ScheduleModal from "./lib/components/ScheduleModal.svelte";
  import NavBarModal from "./lib/components/NavBarModal.svelte";
  import SettingsModal from "./lib/components/SettingsModal.svelte";
  import AccountBar from "./lib/components/shell/AccountBar.svelte";
  import AccountPanel from "./lib/components/shell/AccountPanel.svelte";
  import MenuBar from "./lib/components/shell/MenuBar.svelte";
  import NavBar from "./lib/components/shell/NavBar.svelte";
  import { runAction } from "./lib/shell/actions";
  import { goHome } from "./lib/shell/nav";
  import { MENUS } from "./lib/shell/menus";
  import { dialogState } from "./lib/state/dialogs.svelte";
  import { listsState } from "./lib/state/lists.svelte";
  import { scheduleState } from "./lib/state/schedule.svelte";
  import { settingsState } from "./lib/state/settings.svelte";
  import { themeState } from "./lib/state/theme.svelte";
  import { viewState } from "./lib/state/view.svelte";
  import Calendar from "./views/Calendar.svelte";
  import Dashboard from "./views/Dashboard.svelte";
  import EmptyBook from "./views/EmptyBook.svelte";
  import Manage from "./views/Manage.svelte";
  import Scheduled from "./views/Scheduled.svelte";
  import Search from "./views/Search.svelte";

  onMount(() => {
    void listsState.loadAll().then(() => {
      // Start on the home screen, unless the user has already gone somewhere.
      if (viewState.untouched) void goHome();
      return scheduleState.startup();
    });
  });

  const views: Record<string, Component> = {
    dashboard: Dashboard,
    account: Account,
    accounts: Accounts,
    manage: Manage,
    scheduled: Scheduled,
    calendar: Calendar,
    search: Search,
  };
  const View = $derived(views[viewState.current]);
</script>

<div
  class="app"
  data-theme={themeState.theme}
  style="font-size: {themeState.fontSize}px"
>
  <MenuBar menus={MENUS} onselect={runAction} />
  <NavBar />
  {#if !listsState.isEmptyBook}<AccountBar />{/if}
  {#if listsState.error}<p class="err">{listsState.error}</p>{/if}
  {#if dialogState.account !== undefined}
    {#key dialogState.account?.id ?? "new"}<AccountModal account={dialogState.account} />{/key}
  {/if}
  {#if dialogState.history}<HistoryModal txn={dialogState.history.txn} />{/if}
  {#if dialogState.integrity}<IntegrityModal />{/if}
  {#if dialogState.settings}<SettingsModal />{/if}
  {#if dialogState.navbar}<NavBarModal />{/if}
  {#if dialogState.due}<DueDialog />{/if}
  {#if dialogState.schedule !== undefined}
    {#key dialogState.schedule.id ?? `new-${dialogState.schedule.start}`}
      <ScheduleModal
        id={dialogState.schedule.id}
        fields={dialogState.schedule.fields}
        start={dialogState.schedule.start}
      />
    {/key}
  {/if}
  <ConfirmDialog />
  <div class="body" class:right={settingsState.accountPanelSide === "right"}>
    {#if settingsState.accountPanelOpen && !listsState.isEmptyBook}
      <AccountPanel />
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
  /* The field being typed in must be unmistakable (keyboard entry). */
  :global(input:focus),
  :global(select:focus),
  :global(textarea:focus) {
    outline: 3px solid #1f6feb;
    outline-offset: 0;
    background: #fff6b0;
    color: #111;
    box-shadow: 0 0 0 4px rgba(31, 111, 235, 0.35);
    position: relative;
    z-index: 1;
  }
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
    --bg: #fff;
    /* Problem and OK colors for a red-green colorblind eye: vermilion and
       blue differ in lightness and on the blue-yellow axis, and each sits
       at 7:1 or better on its background. Never the only cue: text or a
       symbol always says the same thing. */
    --bad: #a83200;
    --good: #005a9c;
  }
  .app[data-theme="dark"] {
    --bg: #1e1e1e;
    --bad: #ff9f5a;
    --good: #7cc0ff;
    background: #1e1e1e;
    color: #eee;
  }
  .app[data-theme="light"] {
    background: #fff;
    color: #111;
  }
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .body.right {
    flex-direction: row-reverse;
  }
  main {
    padding: 1rem;
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
  }
  .err {
    color: var(--bad, #a83200);
    margin: 0.5rem;
  }
</style>
