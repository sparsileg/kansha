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
  import Scheduled from "./views/Scheduled.svelte";
  import Calendar from "./views/Calendar.svelte";
  import DueDialog from "./lib/components/DueDialog.svelte";
  import ScheduleModal from "./lib/components/ScheduleModal.svelte";
  import { scheduleState } from "./lib/state/schedule.svelte";
  import { registerState } from "./lib/state/register.svelte";

  onMount(() => {
    void listsState.loadAll().then(() => scheduleState.startup());
  });

  // Placeholder until the other views exist (Phase 3+); Dashboard is the
  // only real one in Phase 0.
  const views: Record<string, Component> = {
    dashboard: Dashboard,
    account: Account,
    manage: Manage,
    scheduled: Scheduled,
    calendar: Calendar,
  };
  const View = $derived(views[viewState.current]);
  /** The account whose register was last open, to get back to from any view. */
  const lastAccount = $derived(
    registerState.accountId === null ? undefined : listsState.account(registerState.accountId),
  );
</script>

<div
  class="app"
  data-theme={themeState.theme}
  style="font-size: {themeState.fontSize}px"
>
  <nav>
    <button onclick={() => viewState.navigate("dashboard")}>Dashboard</button>
    {#if lastAccount}
      <button
        onclick={() => viewState.navigate("account")}
        aria-current={viewState.current === "account" ? "page" : undefined}
      >
        Account: {lastAccount.name}
      </button>
    {/if}
    {#if settingsState.accountNav === "dropdown" && !listsState.isEmptyBook}
      <AccountSelector />
    {/if}
    <button onclick={() => viewState.navigate("scheduled")}>Scheduled</button>
    <button onclick={() => viewState.navigate("calendar")}>Calendar</button>
    <button onclick={() => (dialogState.due = true)}>
      Due{scheduleState.attention > 0 ? ` (${scheduleState.attention})` : ""}
    </button>
    <button onclick={() => viewState.navigate("manage")}>Payees, categories, tags</button>
    <button onclick={() => dialogState.newAccount()}>New account</button>
    <button onclick={() => (dialogState.integrity = true)}>Integrity check</button>
    <label class="closed">
      <input type="checkbox" bind:checked={settingsState.showClosedAccounts} /> Show closed accounts
    </label>
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
  nav {
    flex-wrap: wrap;
    padding: 0.5rem;
    display: flex;
    gap: 0.5rem;
    border-bottom: 1px solid rgba(128, 128, 128, 0.3);
  }
  .closed {
    align-self: center;
    font-size: 0.9em;
  }
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
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
