<script lang="ts">
  import { startupChoices } from "../shell/nav";
  import { dialogState } from "../state/dialogs.svelte";
  import { DATE_FORMATS, dateFormatState, type DateFormat } from "../state/dateformat.svelte";
  import { settingsState, type PanelSide } from "../state/settings.svelte";
  import { FONT_SIZES, themeState, type Theme } from "../state/theme.svelte";
  import Modal from "./Modal.svelte";

  // Reads listsState.accounts, so a new account shows up at once.
  const choices = $derived(startupChoices());
</script>

<Modal title="Settings" onclose={() => (dialogState.settings = false)}>
  <div class="form">
    <label>
      <span>Theme</span>
      <select value={themeState.theme} onchange={(e) => themeState.setTheme(e.currentTarget.value as Theme)}>
        <option value="light">Light</option>
        <option value="dark">Dark</option>
      </select>
    </label>
    <label>
      <span>Font size</span>
      <select value={String(themeState.fontSize)} onchange={(e) => themeState.setFontSize(Number(e.currentTarget.value))}>
        {#each FONT_SIZES as px (px)}<option value={String(px)}>{px} px</option>{/each}
      </select>
    </label>
    <label>
      <span>Date format</span>
      <select value={dateFormatState.value} onchange={(e) => dateFormatState.set(e.currentTarget.value as DateFormat)}>
        {#each DATE_FORMATS as f (f.value)}<option value={f.value}>{f.label}</option>{/each}
      </select>
    </label>
    <label>
      <span>On startup open to:</span>
      <select value={settingsState.startup} onchange={(e) => settingsState.setStartup(e.currentTarget.value)}>
        {#each choices as c (c.value)}<option value={c.value}>{c.label}</option>{/each}
      </select>
    </label>
    <label>
      <span>Account list side</span>
      <select value={settingsState.accountPanelSide} onchange={(e) => settingsState.setAccountPanelSide(e.currentTarget.value as PanelSide)}>
        <option value="left">Left</option>
        <option value="right">Right</option>
      </select>
    </label>
    <p class="note">
      Kept on this computer for now. The startup choice will belong to each book once separate books exist.
    </p>
    <div class="row">
      <button type="button" onclick={() => (dialogState.settings = false)}>Close</button>
    </div>
  </div>
</Modal>

<style>
  /* One line per setting: the label right-aligned on the left, its
     control on the right. Each label's text and control take the two
     columns (`display: contents`), so every control lines up. */
  .form {
    display: grid;
    grid-template-columns: max-content auto;
    gap: 0.5rem 0.75rem;
    align-items: center;
  }
  label {
    display: contents;
  }
  label span {
    text-align: right;
  }
  label select {
    justify-self: start;
  }
  /* Wraps to the width the settings take: `width: 0` keeps the note from
     widening the grid, `min-width: 100%` then fills it. */
  .note {
    grid-column: 1 / -1;
    width: 0;
    min-width: 100%;
    margin: 0;
    opacity: 0.7;
    font-size: 0.85em;
  }
  .row {
    grid-column: 1 / -1;
    display: flex;
    justify-content: flex-end;
  }
</style>
