<script lang="ts">
  import { groupAccounts } from "../state/groups";
  import { dialogState } from "../state/dialogs.svelte";
  import { listsState } from "../state/lists.svelte";
  import { settingsState, type PanelSide } from "../state/settings.svelte";
  import { FONT_SIZES, themeState, type Theme } from "../state/theme.svelte";
  import Modal from "./Modal.svelte";

  const accounts = $derived(groupAccounts(listsState.accounts, false).flatMap((g) => g.accounts));
</script>

<Modal title="Settings" onclose={() => (dialogState.settings = false)}>
  <div class="form">
    <label>
      Theme
      <select value={themeState.theme} onchange={(e) => themeState.setTheme(e.currentTarget.value as Theme)}>
        <option value="light">Light</option>
        <option value="dark">Dark</option>
      </select>
    </label>
    <label>
      Font size
      <select value={String(themeState.fontSize)} onchange={(e) => themeState.setFontSize(Number(e.currentTarget.value))}>
        {#each FONT_SIZES as px (px)}<option value={String(px)}>{px} px</option>{/each}
      </select>
    </label>
    <label>
      Home screen
      <select value={settingsState.home} onchange={(e) => settingsState.setHome(e.currentTarget.value)}>
        <option value="dashboard">Dashboard (placeholder)</option>
        <option value="scheduled">Reminders</option>
        <option value="calendar">Calendar</option>
        {#each accounts as a (a.id)}<option value={`account:${a.id}`}>Account: {a.name}</option>{/each}
      </select>
    </label>
    <label>
      Account list side
      <select value={settingsState.accountPanelSide} onchange={(e) => settingsState.setAccountPanelSide(e.currentTarget.value as PanelSide)}>
        <option value="left">Left</option>
        <option value="right">Right</option>
      </select>
    </label>
    <p class="note">
      Kept on this computer for now. The home screen will belong to each book once separate books exist.
    </p>
    <div class="row">
      <button type="button" onclick={() => (dialogState.settings = false)}>Close</button>
    </div>
  </div>
</Modal>

<style>
  .form {
    display: grid;
    gap: 0.6rem;
    min-width: 20rem;
  }
  label {
    display: grid;
    gap: 0.15rem;
  }
  .note {
    margin: 0;
    opacity: 0.7;
    font-size: 0.85em;
  }
  .row {
    display: flex;
    justify-content: flex-end;
  }
</style>
