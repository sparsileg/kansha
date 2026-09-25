<script lang="ts">
  import type { Component } from "svelte";
  import { onMount } from "svelte";
  import { selectOnFocus } from "./lib/ui/selectOnFocus";
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
  import Investments from "./views/Investments.svelte";
  import EmptyBook from "./views/EmptyBook.svelte";
  import Manage from "./views/Manage.svelte";
  import Reconcile from "./views/Reconcile.svelte";
  import Scheduled from "./views/Scheduled.svelte";
  import Search from "./views/Search.svelte";

  // Every text field selects its contents on focus, so typing replaces it.
  onMount(() => selectOnFocus(document));

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
    reconcile: Reconcile,
    search: Search,
    investments: Investments,
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
  /* The window itself never scrolls: the shell is pinned to it, and only
     areas inside (views, lists, the account panel) scroll. */
  :global(html),
  :global(body) {
    margin: 0;
    height: 100%;
    overflow: hidden;
  }
  /* The field being typed in must be unmistakable (keyboard entry): its
     background and text take the theme's focus colors. Buttons reached by
     keyboard look the same. */
  :global(input:focus),
  :global(select:focus),
  :global(textarea:focus),
  :global(button:focus-visible) {
    /* Drawn inside the edge, so a neighbouring cell cannot cover it. */
    outline: 3px solid var(--focus-ring);
    outline-offset: -3px;
    background: var(--focus-bg);
    color: var(--focus-fg);
    box-shadow: 0 0 0 4px var(--focus-glow);
    position: relative;
    z-index: 1;
  }
  /* A dropdown's open list: the theme's plain colors, never the focus
     colors its field has while open. */
  :global(option),
  :global(optgroup) {
    background: var(--opt-bg);
    color: var(--opt-fg);
  }
  /* A focused field's contents, selected on focus: a stronger shade of
     the focus background, same text color, so the field keeps its look. */
  :global(input:focus::selection),
  :global(textarea:focus::selection) {
    background: var(--focus-sel-bg);
    color: var(--focus-fg);
  }
  /* Selected text elsewhere. */
  :global(::selection) {
    background: var(--sel-bg);
    color: var(--sel-fg);
  }
  .app {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    --bg: #fff;
    /* Problem and OK colors for a red-green colorblind eye: vermilion and
       blue differ in lightness and on the blue-yellow axis, and each sits
       at 7:1 or better on its background. Never the only cue: text or a
       symbol always says the same thing. */
    --bad: #a83200;
    --good: #005a9c;
    /* Focus and selection, per theme, the same for every kind of field.
       Yellow and blue stay apart for a red-green colorblind eye; text on
       each is 7:1 or better. */
    color-scheme: light;
    --opt-bg: #fff;
    --opt-fg: #111;
    --focus-bg: #fff6b0;
    --focus-fg: #111;
    --focus-sel-bg: #ffc933;
    --focus-ring: #1f6feb;
    --focus-glow: rgba(31, 111, 235, 0.35);
    --sel-bg: #1f6feb;
    --sel-fg: #fff;
  }
  .app[data-theme="dark"] {
    --bg: #1e1e1e;
    --bad: #ff9f5a;
    --good: #7cc0ff;
    /* Native controls (fields, dropdown lists, scrollbars) draw dark. */
    color-scheme: dark;
    --opt-bg: #2a2a2a;
    --opt-fg: #eee;
    /* White on deep blue: 10:1. The yellow ring marks it at a glance. */
    --focus-bg: #0b3d91;
    --focus-fg: #fff;
    --focus-sel-bg: #2563d9;
    --focus-ring: #ffd84d;
    --focus-glow: rgba(255, 216, 77, 0.35);
    --sel-bg: #7cc0ff;
    --sel-fg: #111;
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
