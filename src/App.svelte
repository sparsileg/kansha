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
  import Dock from "./lib/components/shell/Dock.svelte";
  import WindowFrame from "./lib/components/shell/WindowFrame.svelte";
  import ReportWindow from "./lib/components/reports/ReportWindow.svelte";
  import SavedReportsModal from "./lib/components/reports/SavedReportsModal.svelte";
  import MenuBar from "./lib/components/shell/MenuBar.svelte";
  import NavBar from "./lib/components/shell/NavBar.svelte";
  import ThemePicker from "./lib/components/shell/ThemePicker.svelte";
  import { runAction } from "./lib/shell/actions";
  import DbKeyModal from "./lib/components/backup/DbKeyModal.svelte";
  import PassphraseModal from "./lib/components/backup/PassphraseModal.svelte";
  import RestoreModal from "./lib/components/backup/RestoreModal.svelte";
  import VerifyBackupModal from "./lib/components/backup/VerifyBackupModal.svelte";
  import Modal from "./lib/components/Modal.svelte";
  import { guardWindowClose } from "./lib/shell/nav";
  import { startBook } from "./lib/shell/startup";
  import { bookState } from "./lib/state/book.svelte";
  import StartScreen from "./views/StartScreen.svelte";
  import { isPanel, type PanelKind } from "./lib/shell/panels";
  import { MENUS } from "./lib/shell/menus";
  import { dialogState } from "./lib/state/dialogs.svelte";
  import { listsState } from "./lib/state/lists.svelte";
  import { REPORT_WINDOW, reportState } from "./lib/state/reports.svelte";
  import { settingsState } from "./lib/state/settings.svelte";
  import { applyTheme, themeState } from "./lib/state/theme.svelte";
  import { viewState } from "./lib/state/view.svelte";
  import { windowState } from "./lib/state/windows.svelte";
  import Calendar from "./views/Calendar.svelte";
  import Dashboard from "./views/Dashboard.svelte";
  import Investments from "./views/Investments.svelte";
  import EmptyBook from "./views/EmptyBook.svelte";
  import Manage from "./views/Manage.svelte";
  import Reconcile from "./views/Reconcile.svelte";
  import Scheduled from "./views/Scheduled.svelte";
  import Search from "./views/Search.svelte";

  // The theme and base font size go on <html> (src/css/themes, base.css).
  $effect(() => applyTheme(document.documentElement, themeState.theme, themeState.fontSize));
  // Every text field selects its contents on focus, so typing replaces it.
  onMount(() => selectOnFocus(document));
  // The close box asks to save changed reports, like File > Exit.
  onMount(() => void guardWindowClose());

  // Nothing loads until a book is open (SECU-020): the start screen shows
  // until then.
  onMount(() => void bookState.refresh());
  let started = false;
  $effect(() => {
    if (bookState.open && !started) {
      started = true;
      void startBook();
    }
  });

  const views: Record<string, Component> = {
    dashboard: Dashboard,
    account: Account,
    manage: Manage,
    search: Search,
    investments: Investments,
  };
  const View = $derived(views[viewState.current]);
  const panels: Record<PanelKind, Component> = {
    calendar: Calendar,
    scheduled: Scheduled,
    accounts: Accounts,
    reconcile: Reconcile,
  };
  /** The window on top, when one is showing. */
  const win = $derived(windowState.shown === null ? undefined : windowState.get(windowState.shown));
  const report = $derived(win?.kind === REPORT_WINDOW ? reportState.get(win.id) : undefined);
</script>

{#if !bookState.open}
<StartScreen />
{:else}
<div class="app">
  <MenuBar menus={MENUS} onselect={runAction}><ThemePicker /></MenuBar>
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
  {#if reportState.savedOpen}
    <SavedReportsModal
      onopen={(r) => {
        reportState.savedOpen = false;
        void reportState.openSaved(r);
      }}
      onclose={() => (reportState.savedOpen = false)}
    />
  {/if}
  <ConfirmDialog />
  {#if dialogState.restore}
    <!-- A restored book starts over: reload, as after unlocking. -->
    <RestoreModal
      start={settingsState.backupFolder}
      onclose={() => (dialogState.restore = false)}
      ondone={() => location.reload()}
    />
  {/if}
  {#if dialogState.verify}
    <VerifyBackupModal start={settingsState.backupFolder} onclose={() => (dialogState.verify = false)} />
  {/if}
  {#if dialogState.passphrase}<PassphraseModal onclose={() => (dialogState.passphrase = false)} />{/if}
  {#if dialogState.dbKey}<DbKeyModal onclose={() => (dialogState.dbKey = false)} />{/if}
  {#if dialogState.backupDone}
    <Modal title="Back up now" onclose={() => (dialogState.backupDone = null)}>
      <p role={dialogState.backupDone.ok ? "status" : "alert"}>{dialogState.backupDone.text}</p>
      <p><button type="button" onclick={() => (dialogState.backupDone = null)}>Close</button></p>
    </Modal>
  {/if}
  <div class="body" class:right={settingsState.accountPanelSide === "right"}>
    {#if settingsState.accountPanelOpen && !listsState.isEmptyBook}
      <AccountPanel />
    {/if}
    <main>
      {#if listsState.isEmptyBook}
        <EmptyBook />
      {:else if win}
        {#key win.id}
          <WindowFrame id={win.id}>
            {#if report}
              <ReportWindow inst={report} />
            {:else if isPanel(win.kind)}
              {@const Panel = panels[win.kind]}
              <Panel />
            {/if}
          </WindowFrame>
        {/key}
      {:else if View}
        <View />
      {/if}
    </main>
  </div>
  <Dock />
</div>
{/if}

<style>
  /* Global element styles are in src/css/base.css; colors and the font
     family in src/css/themes. */
  .app {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
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
    color: var(--bad);
    margin: 0.5rem;
  }
  /* Printing (RPT-050) shows only the view: no menus, bars, or account
     list, and nothing clipped by the fixed window. */
  @media print {
    .app {
      position: static;
      display: block;
    }
    .app > :global(:not(.body)),
    .body > :global(:not(main)),
    :global(.no-print) {
      display: none !important;
    }
    .body,
    main {
      display: block;
      overflow: visible;
      padding: 0;
    }
  }
</style>
