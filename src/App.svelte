<script lang="ts">
  import type { Component } from "svelte";
  import { onMount } from "svelte";
  import { selectOnFocus } from "./lib/ui/selectOnFocus";
  import { installUndoKey } from "./lib/ui/undoKey";
  import Account from "./views/Account.svelte";
  import Accounts from "./views/Accounts.svelte";
  import AccountModal from "./lib/components/AccountModal.svelte";
  import ConfirmDialog from "./lib/components/ConfirmDialog.svelte";
  import DueDialog from "./lib/components/DueDialog.svelte";
  import HistoryModal from "./lib/components/HistoryModal.svelte";
  import AboutModal from "./lib/components/AboutModal.svelte";
  import PriceImportModal from "./lib/components/invest/PriceImportModal.svelte";
  import ImportModal from "./lib/components/import/ImportModal.svelte";
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
  import { runAction, undoLast } from "./lib/shell/actions";
  import DbKeyModal from "./lib/components/backup/DbKeyModal.svelte";
  import PassphraseModal from "./lib/components/backup/PassphraseModal.svelte";
  import RestoreModal from "./lib/components/backup/RestoreModal.svelte";
  import VerifyBackupModal from "./lib/components/backup/VerifyBackupModal.svelte";
  import { guardWindowClose } from "./lib/shell/nav";
  import { startBook } from "./lib/shell/startup";
  import { bookState } from "./lib/state/book.svelte";
  import StartScreen from "./views/StartScreen.svelte";
  import { fullWindow } from "./lib/shell/windowsize";
  import { isPanel, type PanelKind } from "./lib/shell/panels";
  import { MENUS, savedReportItems, type Menu, type MenuItem } from "./lib/shell/menus";
  import { recentItems } from "./lib/shell/books";
  import NewBookModal from "./lib/components/books/NewBookModal.svelte";
  import RenameBookModal from "./lib/components/books/RenameBookModal.svelte";
  import { commands } from "./lib/api";
  import type { RecentBook } from "./lib/types/bindings";
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
  $effect(() => applyTheme(document.documentElement, themeState.theme, themeState.fontSize, themeState.font));
  // Every text field selects its contents on focus, so typing replaces it.
  onMount(() => selectOnFocus(document));
  onMount(() => installUndoKey(document, () => void undoLast()));
  // The close box asks to save changed reports, like File > Exit.
  onMount(() => void guardWindowClose());

  // Nothing loads until a book is open (SECU-020): the start screen shows
  // until then.
  onMount(() => void bookState.refresh());

  // File > Recent lists the recent books; the window title names the book.
  let recent = $state<RecentBook[]>([]);
  $effect(() => {
    if (bookState.open) void commands.bookRecent().then((r) => (recent = r));
  });
  // Reports > Saved Reports lists the folders of saved reports.
  $effect(() => {
    if (bookState.open) void reportState.refreshSaved().catch(() => {});
  });
  function fill(i: MenuItem): MenuItem {
    if (i.id === "file.recent") return { ...i, items: recentItems(recent) };
    if (i.id === "reports.saved_menu" && i.items)
      return { ...i, items: savedReportItems(reportState.folders, reportState.savedList, i.items[i.items.length - 1]) };
    return i;
  }
  const menus = $derived<Menu[]>(MENUS.map((m) => ({ ...m, items: m.items.map(fill) })));
  $effect(() => {
    const name = bookState.status?.name;
    const title = name ? `${name} — Kansha` : "Kansha";
    document.title = title;
    void import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => getCurrentWindow().setTitle(title))
      .catch(() => {
        /* not running inside Tauri */
      });
  });
  $effect(() => {
    if (bookState.open) void fullWindow();
  });
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
  };
  const View = $derived(views[viewState.current]);
  const panels: Record<PanelKind, Component> = {
    calendar: Calendar,
    scheduled: Scheduled,
    accounts: Accounts,
    reconcile: Reconcile,
    investments: Investments,
  };
  /** The window on top, when one is showing. */
  const win = $derived(windowState.shown === null ? undefined : windowState.get(windowState.shown));
  const report = $derived(win?.kind === REPORT_WINDOW ? reportState.get(win.id) : undefined);
</script>

{#if !bookState.open}
<StartScreen />
{:else}
<div class="app" style:--account-panel-w={settingsState.accountPanelCss}>
  <MenuBar {menus} onselect={runAction}>
    <div class="bar-end">
      <span class="book" title={bookState.status?.db_path}>{bookState.status?.name}</span>
      <ThemePicker />
    </div>
  </MenuBar>
  <NavBar />
  <AccountBar />
  {#if listsState.error}<p class="err">{listsState.error}</p>{/if}
  {#if dialogState.account !== undefined}
    {#key dialogState.account?.id ?? "new"}<AccountModal account={dialogState.account} />{/key}
  {/if}
  {#if dialogState.history}<HistoryModal entity={dialogState.history.entity} id={dialogState.history.id} />{/if}
  {#if dialogState.integrity}<IntegrityModal />{/if}
  {#if dialogState.settings}<SettingsModal />{/if}
  {#if dialogState.about}<AboutModal />{/if}
  {#if dialogState.priceImport}<PriceImportModal onclose={() => (dialogState.priceImport = false)} />{/if}
  {#if dialogState.qifImport}<ImportModal onclose={() => (dialogState.qifImport = false)} />{/if}
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
  {#if dialogState.newBook}<NewBookModal onclose={() => (dialogState.newBook = false)} />{/if}
  {#if dialogState.renameBook}<RenameBookModal onclose={() => (dialogState.renameBook = false)} />{/if}
  {#if dialogState.passphrase}<PassphraseModal onclose={() => (dialogState.passphrase = false)} />{/if}
  {#if dialogState.dbKey}<DbKeyModal onclose={() => (dialogState.dbKey = false)} />{/if}
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
  /* The open book's name, muted, left of the theme pickers. */
  .bar-end {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 1rem;
    min-width: 0;
  }
  .book {
    opacity: 0.75;
    font-size: var(--fs-small);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 20rem;
  }
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
